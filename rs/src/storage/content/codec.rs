use super::*;

pub(super) const PROGRESS_BYTES: usize = 93;
pub(super) const FILE_BYTES: usize = 108;

pub(super) fn pair_key(a: Particle, b: Particle) -> [u8; 64] {
    let mut key = [0; 64];
    key[..32].copy_from_slice(&a);
    key[32..].copy_from_slice(&b);
    key
}
pub(super) fn part_key(upload: Upload, index: u64) -> [u8; 40] {
    let mut h = hemera::Hasher::new();
    h.update(b"bbg/content-upload\0");
    h.update(&upload.namespace);
    h.update(&upload.request);
    let mut key = [0; 40];
    key[..32].copy_from_slice(h.finalize().as_bytes());
    key[32..].copy_from_slice(&index.to_be_bytes());
    key
}
pub(super) fn retention_key(namespace: Particle, root: Particle, particle: Particle) -> [u8; 64] {
    let mut h = hemera::Hasher::new();
    h.update(b"bbg/content-retention\0");
    h.update(&namespace);
    h.update(&root);
    pair_key(*h.finalize().as_bytes(), particle)
}
fn spec_bytes(spec: Spec) -> [u8; 76] {
    let mut bytes = [0; 76];
    bytes[..32].copy_from_slice(&spec.particle);
    bytes[32..64].copy_from_slice(&spec.profile);
    bytes[64..72].copy_from_slice(&spec.length.to_le_bytes());
    bytes[72..].copy_from_slice(&spec.part_bytes.to_le_bytes());
    bytes
}
fn decode_spec(bytes: &[u8]) -> Result<Spec> {
    if bytes.len() != 76 {
        return Err(StorageError::Corrupt("content spec length").into());
    }
    let spec = Spec {
        particle: bytes[..32].try_into().unwrap(),
        profile: bytes[32..64].try_into().unwrap(),
        length: u64::from_le_bytes(bytes[64..72].try_into().unwrap()),
        part_bytes: u32::from_le_bytes(bytes[72..].try_into().unwrap()),
    };
    spec.validate()
        .map_err(|_| StorageError::Corrupt("content part size"))?;
    Ok(spec)
}
pub(super) fn decode_progress(bytes: &[u8]) -> Result<Progress> {
    if bytes.len() != PROGRESS_BYTES {
        return Err(StorageError::Corrupt("upload record length").into());
    }
    let spec = decode_spec(&bytes[..76])?;
    let present_parts = u64::from_le_bytes(bytes[76..84].try_into().unwrap());
    let state = match bytes[84] {
        0 => State::Staging,
        1 => State::Sealed,
        2 => State::Cancelled,
        _ => return Err(StorageError::Corrupt("upload state").into()),
    };
    let reclaimed_through = u64::from_le_bytes(bytes[85..].try_into().unwrap());
    if present_parts > spec.parts()
        || reclaimed_through > spec.parts()
        || (state == State::Sealed && present_parts != spec.parts())
        || (state != State::Cancelled && reclaimed_through != 0)
    {
        return Err(StorageError::Corrupt("upload coverage").into());
    }
    Ok(Progress {
        spec,
        present_parts,
        state,
        reclaimed_through,
    })
}
pub(super) fn progress(tx: &Transaction<'_>, upload: Upload) -> Result<Option<Progress>> {
    tx.get(
        Table::Uploads,
        &pair_key(upload.namespace, upload.request),
        PROGRESS_BYTES,
    )?
    .map(|bytes| decode_progress(&bytes))
    .transpose()
}
pub(super) fn put_progress(
    tx: &mut Transaction<'_>,
    upload: Upload,
    value: Progress,
) -> Result<()> {
    let mut bytes = [0; PROGRESS_BYTES];
    bytes[..76].copy_from_slice(&spec_bytes(value.spec));
    bytes[76..84].copy_from_slice(&value.present_parts.to_le_bytes());
    bytes[84] = match value.state {
        State::Staging => 0,
        State::Sealed => 1,
        State::Cancelled => 2,
    };
    bytes[85..].copy_from_slice(&value.reclaimed_through.to_le_bytes());
    Ok(tx.put(
        Table::Uploads,
        &pair_key(upload.namespace, upload.request),
        &bytes,
    )?)
}
pub(super) fn encode_file(info: FileInfo) -> [u8; FILE_BYTES] {
    let mut bytes = [0; FILE_BYTES];
    bytes[..32].copy_from_slice(&info.upload.request);
    bytes[32..].copy_from_slice(&spec_bytes(info.spec));
    bytes
}
pub(super) fn file(
    tx: &Transaction<'_>,
    namespace: Particle,
    particle: Particle,
) -> Result<Option<FileInfo>> {
    tx.get(Table::Files, &pair_key(namespace, particle), FILE_BYTES)?
        .map(|bytes| {
            if bytes.len() != FILE_BYTES {
                return Err(StorageError::Corrupt("file descriptor length").into());
            }
            let spec = decode_spec(&bytes[32..])?;
            if spec.particle != particle {
                return Err(StorageError::Corrupt("file descriptor particle").into());
            }
            Ok(FileInfo {
                spec,
                upload: Upload {
                    namespace,
                    request: bytes[..32].try_into().unwrap(),
                },
            })
        })
        .transpose()
}
pub(super) fn read_part(
    tx: &Transaction<'_>,
    upload: Upload,
    spec: Spec,
    index: u64,
) -> Result<Vec<u8>> {
    let key = part_key(upload, index);
    let bytes = tx
        .get(Table::Parts, &key, MAX_PART_BYTES)?
        .ok_or(Error::Incomplete)?;
    if bytes.len() != spec.part_len(index)? {
        return Err(StorageError::Corrupt("content part length").into());
    }
    let checksum = tx
        .get(Table::PartChecks, &key, 32)?
        .ok_or(Error::Incomplete)?;
    if checksum.as_slice() != hemera::hash(&bytes).as_bytes() {
        return Err(StorageError::Corrupt("content part checksum").into());
    }
    Ok(bytes)
}
