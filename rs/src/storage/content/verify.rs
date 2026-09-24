use super::*;

/// Implemented by a trusted host adapter for the exact bound identity profile.
/// BBG handles bytes and durability; an arbitrary supplied verifier is not an
/// authorization or cryptographic trust boundary for untrusted programs.
pub trait Verifier {
    fn profile(&self) -> Particle;
    fn update(&mut self, bytes: &[u8]) -> Result<()>;
    fn finish(self) -> Result<Particle>;
}

pub struct Verification<V> {
    store: ContentStore,
    upload: Upload,
    spec: Spec,
    verifier: Option<V>,
    next: u64,
    complete: Option<FileInfo>,
}
impl<V: Verifier> Verification<V> {
    pub(super) fn new(store: ContentStore, upload: Upload, spec: Spec, verifier: V) -> Self {
        Self {
            store,
            upload,
            spec,
            verifier: Some(verifier),
            next: 0,
            complete: None,
        }
    }
    pub fn verified_parts(&self) -> u64 {
        self.next
    }

    /// Work is bounded by `max_parts * spec.part_bytes`; memory holds one part.
    /// No database lock is held while the host verifier runs.
    pub fn step(&mut self, max_parts: usize) -> Result<Option<FileInfo>> {
        page_limit(max_parts)?;
        if let Some(info) = self.complete {
            return Ok(Some(info));
        }
        if self.verifier.is_none() {
            return Err(Error::Conflict);
        }
        let end = self
            .next
            .saturating_add(max_parts as u64)
            .min(self.spec.parts());
        while self.next < end {
            let bytes = self
                .store
                .db
                .transaction::<_, Error>(|tx| {
                    let current = progress(tx, self.upload)?.ok_or(Error::Missing)?;
                    if current.state == State::Cancelled {
                        return Err(Error::Cancelled);
                    }
                    if current.spec != self.spec {
                        return Err(Error::Conflict);
                    }
                    read_part(tx, self.upload, self.spec, self.next)
                })?
                .value;
            if let Err(error) = self.verifier.as_mut().unwrap().update(&bytes) {
                self.verifier = None;
                return Err(error);
            }
            self.next += 1;
        }
        if self.next < self.spec.parts() {
            return Ok(None);
        }
        if self.verifier.take().unwrap().finish()? != self.spec.particle {
            return Err(Error::IdentityMismatch);
        }
        let info = self
            .store
            .db
            .transaction::<_, Error>(|tx| {
                writable(tx, &self.upload.namespace)?;
                let mut current = progress(tx, self.upload)?.ok_or(Error::Missing)?;
                if current.state == State::Cancelled {
                    return Err(Error::Cancelled);
                }
                if current.spec != self.spec || current.present_parts != self.spec.parts() {
                    return Err(Error::Conflict);
                }
                let candidate = FileInfo {
                    spec: self.spec,
                    upload: self.upload,
                };
                let info = match file(tx, self.upload.namespace, self.spec.particle)? {
                    Some(existing) => {
                        if existing.spec.profile != self.spec.profile
                            || existing.spec.length != self.spec.length
                        {
                            return Err(Error::Conflict);
                        }
                        existing
                    }
                    None => {
                        tx.put(
                            Table::Files,
                            &pair_key(self.upload.namespace, self.spec.particle),
                            &encode_file(candidate),
                        )?;
                        candidate
                    }
                };
                current.state = State::Sealed;
                put_progress(tx, self.upload, current)?;
                Ok(info)
            })?
            .value;
        self.complete = Some(info);
        Ok(Some(info))
    }
}
