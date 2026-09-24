use super::*;

impl ContentStore {
    /// Enumerate uploads within one authorized namespace. Request IDs are the
    /// exclusive page cursor; appending other uploads does not duplicate rows.
    pub fn uploads(
        &self,
        namespace: Particle,
        after: Option<Particle>,
        limit: usize,
    ) -> Result<Vec<(Upload, Progress)>> {
        page_limit(limit)?;
        let after = after.map(|request| pair_key(namespace, request));
        self.db
            .scan(
                Table::Uploads,
                after.as_ref().map(|k| k.as_slice()),
                &namespace,
                ByteLimits {
                    max_entries: limit,
                    max_bytes: limit * (64 + PROGRESS_BYTES),
                },
            )?
            .into_iter()
            .map(|(key, value)| {
                if key.len() != 64 || key[..32] != namespace {
                    return Err(StorageError::Corrupt("upload key").into());
                }
                Ok((
                    Upload {
                        namespace,
                        request: key[32..].try_into().unwrap(),
                    },
                    decode_progress(&value)?,
                ))
            })
            .collect()
    }

    /// Inspect at most `limit` consecutive positions, starting at `from`.
    pub fn coverage(&self, upload: Upload, from: u64, limit: usize) -> Result<Coverage> {
        page_limit(limit)?;
        self.db
            .transaction::<_, Error>(|tx| {
                let current = progress(tx, upload)?.ok_or(Error::Missing)?;
                if current.state == State::Cancelled {
                    return Err(Error::Cancelled);
                }
                if from > current.spec.parts() {
                    return Err(Error::InvalidRange);
                }
                let end = from.saturating_add(limit as u64).min(current.spec.parts());
                let mut present = Vec::with_capacity((end - from) as usize);
                for index in from..end {
                    let checksum = tx.get(Table::PartChecks, &part_key(upload, index), 32)?;
                    if checksum.as_ref().is_some_and(|c| c.len() != 32) {
                        return Err(StorageError::Corrupt("content coverage checksum").into());
                    }
                    present.push((index, checksum.is_some()));
                }
                Ok(Coverage {
                    present,
                    next: (end < current.spec.parts()).then_some(end),
                })
            })
            .map(|c| c.value)
    }

    /// Read a sealed range with local checksum verification. This is not a
    /// network range proof. Canonical sealed content is protected from cancel.
    pub fn read_range(
        &self,
        namespace: Particle,
        particle: Particle,
        profile: Particle,
        offset: u64,
        max_bytes: usize,
    ) -> Result<Vec<u8>> {
        if max_bytes > MAX_PART_BYTES {
            return Err(StorageError::Limit("content read budget").into());
        }
        let info = self.file(namespace, particle)?.ok_or(Error::Missing)?;
        if info.spec.profile != profile {
            return Err(Error::ProfileMismatch);
        }
        if offset > info.spec.length {
            return Err(Error::InvalidRange);
        }
        let count = (info.spec.length - offset).min(max_bytes as u64) as usize;
        let mut output = Vec::with_capacity(count);
        let mut position = offset;
        while output.len() < count {
            let index = position / u64::from(info.spec.part_bytes);
            let bytes = self
                .db
                .transaction::<_, Error>(|tx| read_part(tx, info.upload, info.spec, index))?
                .value;
            let start = (position % u64::from(info.spec.part_bytes)) as usize;
            let take = (count - output.len()).min(bytes.len() - start);
            output.extend_from_slice(&bytes[start..start + take]);
            position += take as u64;
        }
        Ok(output)
    }

    pub fn is_retained(
        &self,
        namespace: Particle,
        particle: Particle,
        profile: Particle,
        root: Particle,
    ) -> Result<bool> {
        let value = self.db.read(
            Table::RetainedContent,
            &retention_key(namespace, root, particle),
            32,
        )?;
        match value {
            Some(value) if value.as_slice() == profile => Ok(true),
            Some(_) => Err(Error::ProfileMismatch),
            None => Ok(false),
        }
    }
}
