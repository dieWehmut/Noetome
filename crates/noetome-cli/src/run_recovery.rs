//! Read-only gate for the owned Run database's SQLite rollback journal.
//!
//! Protocol: sqlite.org/fileformat.html#the_rollback_journal and the locked
//! libsqlite3-sys 0.39.1 SQLite 3.54.1 pager.c amalgamation (readJournalHdr,
//! pager_cksum, pager_playback, writeSuperJournal). No journal is removed or
//! rewritten here. Only SQLite, after this gate, performs disk recovery.
//!
//! A rollback journal does not contain a database file identity. Its sparse
//! checksum is not authentication. Therefore structure alone is insufficient:
//! the before-images are applied to a bounded memory image and the original
//! Owner, metadata, Run/Release IDs, chain and selected inputs are checked there
//! before the caller opens the real database for writing. The caller must hold
//! the worker lock and repeat complete Run/output validation after recovery.

use std::collections::BTreeSet;
use std::fs::{File, Metadata};
use std::io::{Cursor, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use rusqlite::Connection;

use crate::compile_identity::{Identity, identity};
use crate::project::{self, ProjectError};
use crate::run_model::{Checkpoint, Owner, POLICY, RunMetadata};
use crate::text::sha256;

const MAGIC: &[u8; 8] = b"\xd9\xd5\x05\xf9\x20\xa1\x63\xd7";
const LIMIT: usize = 64 * 1024 * 1024;
const JSON_LIMIT: usize = 8 * 1024 * 1024;
const APP: u32 = 0x4e52554e;
const COMPILE_APP: u32 = 0x4e434d50;

fn invalid(message: impl Into<String>) -> ProjectError {
    ProjectError::new("RUN_RECOVERY_REQUIRED", 3, message)
}

/// This function requires an already validated Run owner and exclusive worker
/// lock. Call it before opening a writable DB handle (also on Windows).
pub(crate) fn validate_hot_journal(db: &Path, journal: &Path) -> Result<(), ProjectError> {
    exact_journal_path(db, journal)?;
    if db.file_name().is_none_or(|name| name != "run.sqlite")
        || db
            .parent()
            .and_then(Path::file_name)
            .is_none_or(|name| name != "control")
    {
        return Err(invalid(
            "恢复只允许已拥有control/run.sqlite的同目录精确-journal路径。",
        ));
    }
    let mut database = GuardedBytes::open(db, LIMIT)?;
    let mut rollback = GuardedBytes::open(journal, LIMIT)?;
    let control = db.parent().ok_or_else(|| invalid("缺少control目录。"))?;
    let root = control.parent().ok_or_else(|| invalid("缺少Run根目录。"))?;
    let mut owner_file = GuardedBytes::open(&control.join("owner.json"), JSON_LIMIT)?;
    let owner: Owner = serde_json::from_slice(&owner_file.bytes)
        .map_err(|e| invalid(format!("恢复前原所有权格式无效：{e}")))?;
    validate_owner(root, control, &owner, &database.id)?;

    let image = rollback_image(&database.bytes, &rollback.bytes)?;
    validate_shadow(image, &owner, &owner_file.id)?;

    // Keep all ordinary read handles alive until every semantic check finishes.
    // Identity, byte content and metadata are rechecked, not merely cached.
    validate_owner(root, control, &owner, &database.id)?;
    database.check()?;
    rollback.check()?;
    owner_file.check()?;
    Ok(())
}

/// Produce a read-only original DB image for a caller with an independent
/// durable snapshot anchor. This is not permission to recover: the caller must
/// match its original database identity and the *entire* returned image SHA to
/// its existing snapshot before any real SQLite open that may write. Without
/// that anchor (including bootstrap with no base snapshot), refuse recovery.
pub(crate) fn checked_recovery_image(db: &Path, journal: &Path) -> Result<Vec<u8>, ProjectError> {
    exact_journal_path(db, journal)?;
    let mut database = GuardedBytes::open(db, LIMIT)?;
    let mut rollback = GuardedBytes::open(journal, LIMIT)?;
    let image = rollback_image(&database.bytes, &rollback.bytes)?;
    database.check()?;
    rollback.check()?;
    Ok(image.bytes)
}

#[derive(PartialEq, Eq)]
struct FileWitness {
    path: PathBuf,
    identity: Identity,
    sha256: String,
    byte_length: u64,
    modified: SystemTime,
}

/// No file handles remain held by this witness. The caller must verify it
/// immediately before normal SQLite open, after all semantic checks. On
/// Windows the read handles must close before a writable SQLite handle opens.
pub(crate) struct RecoveryWitness {
    database: FileWitness,
    rollback: FileWitness,
}

impl RecoveryWitness {
    pub(crate) fn capture(db: &Path, journal: &Path) -> Result<Self, ProjectError> {
        exact_journal_path(db, journal)?;
        let mut database = GuardedBytes::open(db, LIMIT)?;
        let mut rollback = GuardedBytes::open(journal, LIMIT)?;
        database.check()?;
        rollback.check()?;
        Ok(Self {
            database: database.witness()?,
            rollback: rollback.witness()?,
        })
    }

    pub(crate) fn verify(&self, db: &Path, journal: &Path) -> Result<(), ProjectError> {
        if self.database.path != db || self.rollback.path != journal {
            return Err(invalid("恢复witness不是原先精确DB/journal路径。"));
        }
        let current = Self::capture(db, journal)?;
        if current.database != self.database || current.rollback != self.rollback {
            return Err(invalid(
                "恢复语义校验期间DB/journal身份、SHA、长度或修改时间改变。",
            ));
        }
        Ok(())
    }
}

fn exact_journal_path(db: &Path, journal: &Path) -> Result<(), ProjectError> {
    let mut expected_name = db.as_os_str().to_os_string();
    expected_name.push("-journal");
    if journal != Path::new(&expected_name) || db.parent() != journal.parent() {
        return Err(invalid("恢复仅允许精确原DB路径对应的同目录-journal。"));
    }
    Ok(())
}

struct GuardedBytes {
    path: PathBuf,
    file: File,
    opened: same_file::Handle,
    id: Identity,
    metadata: Metadata,
    bytes: Vec<u8>,
}

impl GuardedBytes {
    fn witness(&self) -> Result<FileWitness, ProjectError> {
        Ok(FileWitness {
            path: self.path.clone(),
            identity: self.id.clone(),
            sha256: sha256(&self.bytes),
            byte_length: self.bytes.len() as u64,
            modified: self
                .metadata
                .modified()
                .map_err(|e| ProjectError::io(&self.path, e))?,
        })
    }
    fn open(path: &Path, limit: usize) -> Result<Self, ProjectError> {
        project::check_components(path)?;
        project::checked_file(path)?;
        let id = identity(path)?;
        let initial = same_file::Handle::from_path(path).map_err(|e| ProjectError::io(path, e))?;
        let mut file = project::read_guard(path)?;
        let metadata = file.metadata().map_err(|e| ProjectError::io(path, e))?;
        if !metadata.is_file() || metadata.len() > limit as u64 {
            return Err(invalid(
                "恢复文件不是有限的普通文件；DB/journal最多64MiB，owner最多8MiB。",
            ));
        }
        let opened =
            same_file::Handle::from_file(file.try_clone().map_err(|e| ProjectError::io(path, e))?)
                .map_err(|e| ProjectError::io(path, e))?;
        if opened != initial || id != identity(path)? {
            return Err(invalid("恢复文件在打开时身份改变；保留现场。"));
        }
        let mut bytes = Vec::new();
        Read::by_ref(&mut file)
            .take(limit as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| ProjectError::io(path, e))?;
        if bytes.len() > limit || bytes.len() as u64 != metadata.len() {
            return Err(invalid("恢复文件在读取时改变或超过限制。"));
        }
        let mut result = Self {
            path: path.into(),
            file,
            opened,
            id,
            metadata,
            bytes,
        };
        result.check()?;
        Ok(result)
    }

    fn check(&mut self) -> Result<(), ProjectError> {
        project::check_components(&self.path)?;
        project::checked_file(&self.path)?;
        let current = same_file::Handle::from_path(&self.path)
            .map_err(|e| ProjectError::io(&self.path, e))?;
        let metadata = self
            .file
            .metadata()
            .map_err(|e| ProjectError::io(&self.path, e))?;
        if current != self.opened
            || identity(&self.path)? != self.id
            || metadata.len() != self.metadata.len()
            || metadata
                .modified()
                .map_err(|e| ProjectError::io(&self.path, e))?
                != self
                    .metadata
                    .modified()
                    .map_err(|e| ProjectError::io(&self.path, e))?
        {
            return Err(invalid("恢复校验期间文件身份或元数据改变；保留现场。"));
        }
        self.file
            .seek(SeekFrom::Start(0))
            .map_err(|e| ProjectError::io(&self.path, e))?;
        let mut current_bytes = Vec::new();
        Read::by_ref(&mut self.file)
            .take(self.bytes.len() as u64 + 1)
            .read_to_end(&mut current_bytes)
            .map_err(|e| ProjectError::io(&self.path, e))?;
        if current_bytes != self.bytes {
            return Err(invalid("恢复校验期间文件内容改变；保留现场。"));
        }
        Ok(())
    }
}

fn validate_owner(
    root: &Path,
    control: &Path,
    owner: &Owner,
    db_id: &Identity,
) -> Result<(), ProjectError> {
    if owner.schema_version != 1
        || owner.producer != "fixture_run"
        || owner.policy_version != POLICY
        || owner.root != root.to_string_lossy()
        || owner.compiler_fingerprint != env!("NOETOME_RUN_COMPILER_FINGERPRINT")
        || identity(root)? != owner.root_identity
        || identity(root.parent().ok_or_else(|| invalid("Run根没有父目录。"))?)?
            != owner.parent_identity
        || identity(control)? != owner.control_identity
        || *db_id != owner.database_identity
        || identity(&control.join("run.sqlite"))? != owner.database_identity
        || identity(&control.join("worker.lock"))? != owner.lock_identity
        || identity(Path::new(&owner.case_root))? != owner.case_identity
    {
        return Err(invalid("热日志恢复前原Run所有权或实际文件/目录身份不符。"));
    }
    Ok(())
}

fn be32(bytes: &[u8], offset: usize) -> Result<u32, ProjectError> {
    let value = bytes
        .get(offset..offset + 4)
        .ok_or_else(|| invalid("SQLite结构被截断。"))?;
    Ok(u32::from_be_bytes(
        value
            .try_into()
            .map_err(|_| invalid("SQLite整数格式无效。"))?,
    ))
}

fn database_page_size(bytes: &[u8]) -> Result<usize, ProjectError> {
    if bytes.len() < 100 || &bytes[..16] != b"SQLite format 3\0" {
        return Err(invalid("原DB缺少真实SQLite3文件header。"));
    }
    let encoded = u16::from_be_bytes([bytes[16], bytes[17]]);
    let page_size = if encoded == 1 {
        65536
    } else {
        usize::from(encoded)
    };
    if !(512..=65536).contains(&page_size)
        || !page_size.is_power_of_two()
        || bytes.len() < page_size
        || bytes.len() % page_size != 0
        || bytes[18] != 1
        || bytes[19] != 1
        || bytes[20] != 0
        || bytes[21..24] != [64, 32, 32]
        || be32(bytes, 44)? != 4
        || be32(bytes, 56)? != 1
        || be32(bytes, 60)? != 1
        || !matches!(be32(bytes, 68)?, APP | COMPILE_APP)
        || bytes[72..92].iter().any(|b| *b != 0)
    {
        return Err(invalid(
            "DB header不是拥有的有限UTF8/DELETE/Schema1 SQLite格式。",
        ));
    }
    Ok(page_size)
}

fn align(offset: usize, sector: usize) -> Result<usize, ProjectError> {
    offset
        .checked_add(sector - 1)
        .map(|n| n & !(sector - 1))
        .ok_or_else(|| invalid("journal扇区偏移溢出。"))
}

fn checksum(page: &[u8], nonce: u32) -> u32 {
    // Locked SQLite pager_cksum uses unsigned wrapping sums at N-200, N-400,
    // ... while index > 0. Power-of-two SQLite page sizes never reach index 0.
    let mut sum = nonce;
    let mut offset = page.len().saturating_sub(200);
    while offset > 0 {
        sum = sum.wrapping_add(u32::from(page[offset]));
        if offset < 200 {
            break;
        }
        offset -= 200;
    }
    sum
}

fn header_padding(header: &[u8], page_size: usize, count: u32) -> Result<(), ProjectError> {
    // writeJournalHdr fills one sector by repeatedly writing a buffer of
    // min(pageSize, sectorSize) bytes. With FULL sync its initial first 12 bytes
    // are zero; syncJournal later updates only the sector's first magic/count.
    // Thus sector > pageSize has initialized header replicas in the padding,
    // rather than the all-zero padding described by the format overview.
    let chunk_size = page_size.min(header.len());
    let mut initial = header[..28].to_vec();
    if count != u32::MAX {
        initial[..12].fill(0);
    }
    for (index, chunk) in header.chunks_exact(chunk_size).enumerate() {
        if (index > 0 && chunk[..28] != initial) || chunk[28..].iter().any(|b| *b != 0) {
            return Err(invalid(
                "journal header扇区padding或SQLite初始化buffer副本无效。",
            ));
        }
    }
    Ok(())
}

/// Validate all bytes and copy only the synced before-images SQLite will play
/// back. A first zero header is cold and refused. A final initialized but
/// unsynced zero-magic segment may follow a valid hot prefix; its complete
/// records are checked but never copied into the recovery image.
struct RecoveryImage {
    bytes: Vec<u8>,
    restored_pages: BTreeSet<usize>,
}

fn rollback_image(database: &[u8], journal: &[u8]) -> Result<RecoveryImage, ProjectError> {
    let page_size = database_page_size(database)?;
    if journal.len() <= 512 || journal.get(..8) != Some(MAGIC.as_slice()) {
        return Err(invalid(
            "journal缺少非零完整热header；冷日志或外来侧文件保留原样。",
        ));
    }
    // writeSuperJournal appends MAGIC after its filename/length/checksum. Run
    // never ATTACHes databases; do not follow or take ownership of that name.
    if journal.get(journal.len().saturating_sub(8)..) == Some(MAGIC.as_slice()) {
        return Err(invalid("不允许Run日志包含super-journal尾部。"));
    }
    let original_pages = be32(journal, 16)? as usize;
    let sector = be32(journal, 20)? as usize;
    if original_pages == 0
        || !(32..=65536).contains(&sector)
        || !sector.is_power_of_two()
        || be32(journal, 24)? as usize != page_size
    {
        return Err(invalid("journal原DB页数、page size或sector size无效。"));
    }
    let original_size = original_pages
        .checked_mul(page_size)
        .filter(|size| *size <= LIMIT && *size <= database.len())
        .ok_or_else(|| invalid("journal原DB范围越界或超过64MiB。"))?;
    let mut image = database[..original_size].to_vec();
    let record_size = page_size + 8;
    let pending_page = 0x40000000usize / page_size + 1;
    let mut pages = BTreeSet::new();
    let mut restored_pages = BTreeSet::new();
    let mut offset = 0usize;
    let mut applied = 0usize;

    loop {
        let header_end = offset
            .checked_add(sector)
            .filter(|end| *end <= journal.len())
            .ok_or_else(|| invalid("journal扇区header被截断。"))?;
        let magic = &journal[offset..offset + 8];
        let cold_tail = magic.iter().all(|b| *b == 0);
        if magic != MAGIC && !(offset > 0 && cold_tail) {
            return Err(invalid("journal分段magic无效。"));
        }
        let count = be32(journal, offset + 8)?;
        let nonce = be32(journal, offset + 12)?;
        if be32(journal, offset + 16)? as usize != original_pages
            || be32(journal, offset + 20)? as usize != sector
            || be32(journal, offset + 24)? as usize != page_size
            || (cold_tail && count != 0)
            || (!cold_tail && count == 0)
            || (count == u32::MAX && offset != 0)
        {
            return Err(invalid(
                "journal分段字段、记录数或零padding不符合锁定SQLite协议。",
            ));
        }
        header_padding(&journal[offset..header_end], page_size, count)?;
        let records = if cold_tail || count == u32::MAX {
            let remainder = journal.len() - header_end;
            if remainder % record_size != 0 {
                return Err(invalid("journal终末页记录被截断或有未知尾部。"));
            }
            remainder / record_size
        } else {
            count as usize
        };
        if records > original_pages || pages.len().saturating_add(records) > original_pages {
            return Err(invalid("journal记录数量超过原DB页数。"));
        }
        let end = records
            .checked_mul(record_size)
            .and_then(|n| header_end.checked_add(n))
            .filter(|end| *end <= journal.len())
            .ok_or_else(|| invalid("journal声明记录被截断。"))?;
        for index in 0..records {
            let record = header_end + index * record_size;
            let page_number = be32(journal, record)? as usize;
            if page_number == 0
                || page_number == pending_page
                || page_number > original_pages
                || !pages.insert(page_number)
            {
                return Err(invalid("journal页号无效、重复或属于super-journal保留页。"));
            }
            let page = &journal[record + 4..record + 4 + page_size];
            if checksum(page, nonce) != be32(journal, record + 4 + page_size)? {
                return Err(invalid(
                    "journal页记录checksum不符；不让SQLite忽略损坏后继续写。",
                ));
            }
            if page_number == 1
                && (database_page_size(page)? != page_size
                    || be32(page, 68)? != be32(database, 68)?)
            {
                return Err(invalid("journal原page1不属于当前Run SQLite格式。"));
            }
            if !cold_tail {
                let target = (page_number - 1) * page_size;
                image[target..target + page_size].copy_from_slice(page);
                restored_pages.insert(page_number);
                applied += 1;
            }
        }
        if cold_tail || count == u32::MAX || end == journal.len() {
            break;
        }
        let next = align(end, sector)?;
        let padding_end = next.min(journal.len());
        if journal[end..padding_end].iter().any(|b| *b != 0) {
            return Err(invalid("journal分段对齐padding包含未知字节。"));
        }
        if next >= journal.len() {
            break;
        }
        offset = next;
    }
    if applied == 0 {
        return Err(invalid("journal没有可验证的已同步原页记录。"));
    }
    database_page_size(&image)?;
    Ok(RecoveryImage {
        bytes: image,
        restored_pages,
    })
}

fn varint(bytes: &[u8], offset: &mut usize) -> Result<u64, ProjectError> {
    let mut value = 0u64;
    for index in 0..9 {
        let byte = *bytes
            .get(*offset)
            .ok_or_else(|| invalid("metadata cell varint被截断。"))?;
        *offset += 1;
        if index == 8 {
            return Ok((value << 8) | u64::from(byte));
        }
        value = (value << 7) | u64::from(byte & 0x7f);
        if byte & 0x80 == 0 {
            return Ok(value);
        }
    }
    Err(invalid("metadata cell varint无效。"))
}

fn image_page(image: &[u8], page_size: usize, page_number: usize) -> Result<&[u8], ProjectError> {
    let start = page_number
        .checked_sub(1)
        .and_then(|page| page.checked_mul(page_size))
        .ok_or_else(|| invalid("metadata原页号越界。"))?;
    image
        .get(start..start + page_size)
        .ok_or_else(|| invalid("metadata原页不在恢复镜像中。"))
}

/// The one metadata row must itself come entirely from synced before-images.
/// The identity cannot be borrowed from unchanged target DB pages or from an
/// ignored unsynced journal tail. This deliberately rejects unbound journals.
/// Local/overflow lengths and varints follow SQLite's documented table-leaf
/// cell format; quick_check and the complete schema gate run before this.
fn metadata_record_pages(
    image: &[u8],
    root_page: usize,
    raw: &str,
) -> Result<BTreeSet<usize>, ProjectError> {
    let page_size = database_page_size(image)?;
    let page = image_page(image, page_size, root_page)?;
    let header = if root_page == 1 { 100 } else { 0 };
    let cells = u16::from_be_bytes([page[header + 3], page[header + 4]]);
    if page[header] != 0x0d || cells != 1 {
        return Err(invalid("原metadata必须是一个实际table-leaf单例cell。"));
    }
    let mut cell = usize::from(u16::from_be_bytes([page[header + 8], page[header + 9]]));
    if cell < header + 10 {
        return Err(invalid("metadata cell pointer与leaf header重叠。"));
    }
    let payload_size = usize::try_from(varint(page, &mut cell)?)
        .map_err(|_| invalid("metadata payload长度越界。"))?;
    if payload_size > JSON_LIMIT + 32 || varint(page, &mut cell)? != 1 {
        return Err(invalid("metadata payload不是有限singleton=1 record。"));
    }
    let max_local = page_size - 35;
    let min_local = ((page_size - 12) * 32 / 255) - 23;
    let local = if payload_size <= max_local {
        payload_size
    } else {
        let candidate = min_local + ((payload_size - min_local) % (page_size - 4));
        if candidate <= max_local {
            candidate
        } else {
            min_local
        }
    };
    let mut payload = page
        .get(cell..cell + local)
        .ok_or_else(|| invalid("metadata local payload被截断。"))?
        .to_vec();
    let mut pages = BTreeSet::from([root_page]);
    if payload_size > local {
        let mut overflow = be32(page, cell + local)? as usize;
        let mut remaining = payload_size - local;
        while remaining > 0 {
            if !pages.insert(overflow) {
                return Err(invalid("metadata overflow链重复或成环。"));
            }
            let page = image_page(image, page_size, overflow)?;
            let length = remaining.min(page_size - 4);
            payload.extend_from_slice(&page[4..4 + length]);
            remaining -= length;
            overflow = be32(page, 0)? as usize;
        }
        if overflow != 0 {
            return Err(invalid("metadata overflow链有未知额外原页。"));
        }
    }
    let mut offset = 0usize;
    let header_size = usize::try_from(varint(&payload, &mut offset)?)
        .map_err(|_| invalid("metadata record header越界。"))?;
    if header_size > 32 || header_size > payload.len() {
        return Err(invalid("metadata record header长度无效。"));
    }
    let record_header = &payload[..header_size];
    let singleton = varint(record_header, &mut offset)?;
    let schema = varint(record_header, &mut offset)?;
    let text = varint(record_header, &mut offset)?;
    if offset != header_size
        || singleton != 0
        || schema != 9
        || text < 13
        || text % 2 != 1
        || (text - 13) / 2 != raw.len() as u64
        || payload.len() != header_size + raw.len()
        || payload[header_size..] != *raw.as_bytes()
    {
        return Err(invalid(
            "原metadata实际SQLite record不等于已独立解码的完整UTF8元数据。",
        ));
    }
    Ok(pages)
}

fn shadow_error(error: impl std::fmt::Display) -> ProjectError {
    invalid(format!(
        "只读恢复镜像校验失败，真实DB/journal未写入：{error}"
    ))
}

fn validate_shadow(
    recovery: RecoveryImage,
    owner: &Owner,
    owner_id: &Identity,
) -> Result<(), ProjectError> {
    if database_page_size(&recovery.bytes)? < 512 || be32(&recovery.bytes, 68)? != APP {
        return Err(shadow_error(
            "control恢复镜像必须保留原Run rollback header。",
        ));
    }
    // Official deserialize documentation advises 20 spare bytes for uncertain
    // images. The safe rusqlite reader allocates its exact input size, so supply
    // that zero suffix too. The DB header retains its original page count; the
    // incomplete final physical page contains no database page.
    let mut image = recovery.bytes.clone();
    image.extend_from_slice(&[0; 20]);
    let byte_length = image.len();
    let mut connection = Connection::open_in_memory().map_err(shadow_error)?;
    connection
        .deserialize_read_exact("main", Cursor::new(image), byte_length, true)
        .map_err(shadow_error)?;
    connection
        .execute_batch("PRAGMA query_only=ON; PRAGMA trusted_schema=OFF; PRAGMA temp_store=MEMORY;")
        .map_err(shadow_error)?;
    let mut check = connection
        .prepare("PRAGMA quick_check")
        .map_err(shadow_error)?;
    let results = check
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(shadow_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(shadow_error)?;
    if results != ["ok"] {
        return Err(shadow_error("恢复镜像SQLite quick_check未通过。"));
    }
    drop(check);
    crate::run_store::validate_recovery_chain(&connection, &owner.run_id)?;
    let mut schema = connection
        .prepare("SELECT type,name FROM sqlite_schema ORDER BY name")
        .map_err(shadow_error)?;
    let actual = schema
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(shadow_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(shadow_error)?;
    if actual
        != [
            ("table".into(), "checkpoints".into()),
            ("table".into(), "controls".into()),
            ("table".into(), "metadata".into()),
        ]
    {
        return Err(shadow_error("恢复镜像不是Run的三个实际普通表。"));
    }
    drop(schema);
    let count: u32 = connection
        .query_row("SELECT count(*) FROM metadata", [], |row| row.get(0))
        .map_err(shadow_error)?;
    let raw: String = connection
        .query_row(
            "SELECT metadata_json FROM metadata WHERE singleton=1 AND schema_version=1",
            [],
            |row| row.get(0),
        )
        .map_err(shadow_error)?;
    if count != 1 || raw.len() > JSON_LIMIT {
        return Err(shadow_error("恢复镜像metadata不是有限单例。"));
    }
    let metadata: RunMetadata = serde_json::from_str(&raw).map_err(shadow_error)?;
    if metadata.owner != *owner
        || metadata.owner_file_identity != *owner_id
        || metadata.context.run_id != owner.run_id
        || metadata.context.release_id != owner.release_id
        || metadata.inputs.len() < 4
    {
        return Err(shadow_error("恢复镜像属于其他Run或原所有权已改变。"));
    }
    let root_page: u32 = connection
        .query_row(
            "SELECT rootpage FROM sqlite_schema WHERE type='table' AND name='metadata'",
            [],
            |row| row.get(0),
        )
        .map_err(shadow_error)?;
    let metadata_pages = metadata_record_pages(&recovery.bytes, root_page as usize, &raw)?;
    if !metadata_pages.is_subset(&recovery.restored_pages) {
        return Err(shadow_error(
            "热journal没有完整已同步的原metadata record身份页；unbound日志保留，不借用未覆盖DB身份。",
        ));
    }
    let mut statement = connection.prepare("SELECT sequence,previous_sha256,payload_json,payload_sha256 FROM checkpoints ORDER BY sequence")
        .map_err(shadow_error)?;
    let mut rows = statement.query([]).map_err(shadow_error)?;
    let mut sequence = 0u64;
    let mut previous = String::new();
    while let Some(row) = rows.next().map_err(shadow_error)? {
        let seq = u64::from(row.get::<_, u32>(0).map_err(shadow_error)?);
        let prev: String = row.get(1).map_err(shadow_error)?;
        let payload: String = row.get(2).map_err(shadow_error)?;
        let hash: String = row.get(3).map_err(shadow_error)?;
        if seq != sequence + 1
            || seq > 512
            || prev != previous
            || payload.len() > JSON_LIMIT
            || hash != sha256(payload.as_bytes())
        {
            return Err(shadow_error("恢复镜像检查点链失配。"));
        }
        let cp: Checkpoint = serde_json::from_str(&payload).map_err(shadow_error)?;
        if cp.schema_version != 1 || cp.sequence != seq || cp.run_id != owner.run_id {
            return Err(shadow_error("恢复镜像检查点属于其他Run。"));
        }
        sequence = seq;
        previous = hash;
    }
    if sequence == 0 {
        return Err(shadow_error("恢复镜像没有原created检查点。"));
    }
    drop(rows);
    drop(statement);
    let mut controls = connection
        .prepare("SELECT sequence,run_id,action FROM controls ORDER BY sequence")
        .map_err(shadow_error)?;
    let mut rows = controls.query([]).map_err(shadow_error)?;
    let mut sequence = 0u64;
    while let Some(row) = rows.next().map_err(shadow_error)? {
        let seq = u64::from(row.get::<_, u32>(0).map_err(shadow_error)?);
        let run_id: String = row.get(1).map_err(shadow_error)?;
        let action: String = row.get(2).map_err(shadow_error)?;
        if seq != sequence + 1
            || seq > 128
            || run_id != owner.run_id
            || !matches!(action.as_str(), "pause" | "cancel")
        {
            return Err(shadow_error("恢复镜像控制请求身份或顺序不符。"));
        }
        sequence = seq;
    }

    let case = crate::fixture_input::load(Path::new(&owner.case_root))?;
    if case.root().to_string_lossy() != owner.case_root
        || identity(case.root())? != owner.case_identity
        || crate::run_store::inputs(&case)? != metadata.inputs
    {
        return Err(ProjectError::new(
            "RUN_INPUT_INVALIDATED",
            3,
            "恢复前原选取输入与镜像元数据失配；真实DB未写入。",
        ));
    }
    let current = crate::fixture_compile::context(&case)?;
    let expected = &metadata.context;
    if current.case_id != expected.case_id
        || current.fixture_scope_id != expected.fixture_scope_id
        || current.input_hashes != expected.input_hashes
        || current.versions != expected.versions
        || current.counts != expected.counts
        || current.ir_revision != expected.ir_revision
        || current.plan_revision != expected.plan_revision
    {
        return Err(ProjectError::new(
            "RUN_INPUT_INVALIDATED",
            3,
            "恢复前原契约、分母、版本或SHA失配；真实DB未写入。",
        ));
    }
    case.check_unchanged()?;
    let state = crate::run_store::recovery_state(&connection, &owner.run_id)?;
    crate::fixture_run::verify_recovery_outputs(
        &case,
        Path::new(&owner.root),
        &metadata.context,
        &state,
    )?;
    case.check_unchanged()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::run_store::Journal;
    use std::fs;

    struct TestRun {
        base: PathBuf,
        root: PathBuf,
        db: PathBuf,
        journal: PathBuf,
        journal_bytes: Vec<u8>,
        original_db: Vec<u8>,
    }

    impl Drop for TestRun {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.base);
        }
    }

    fn copy_tree(source: &Path, target: &Path) {
        fs::create_dir(target).unwrap();
        for entry in fs::read_dir(source).unwrap() {
            let entry = entry.unwrap();
            assert!(!project::is_link(&entry.metadata().unwrap()));
            if entry.file_type().unwrap().is_dir() {
                copy_tree(&entry.path(), &target.join(entry.file_name()));
            } else {
                fs::copy(entry.path(), target.join(entry.file_name())).unwrap();
            }
        }
    }

    // Generate a real, FULL/DELETE SQLite journal by forcing cache spill. Freeze
    // its DB/journal bytes while the transaction is active, then close SQLite
    // normally and restore those bytes into the same owned DB identity. This
    // tests SQLite's actual on-disk protocol without manually fabricating a
    // magic header and without retaining a writer when the Windows gate opens.
    fn actual_journal() -> TestRun {
        actual_journal_with_page_size(None)
    }

    fn actual_journal_with_page_size(page_size: Option<usize>) -> TestRun {
        actual_journal_with_options(page_size, true)
    }

    fn actual_journal_with_options(page_size: Option<usize>, bound_metadata: bool) -> TestRun {
        let base =
            std::env::temp_dir().join(format!("noetome-run-journal-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&base).unwrap();
        let base = fs::canonicalize(base).unwrap();
        let fixture = base.join("case");
        copy_tree(
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/run-checkpoint-v1/fixture"),
            &fixture,
        );
        let case = crate::fixture_input::load(&fixture).unwrap();
        let context = crate::fixture_compile::context(&case).unwrap();
        let root = base.join("run");
        drop(Journal::create(&case, &root, context, false).unwrap());
        drop(case);
        let db = root.join("control/run.sqlite");
        let journal = root.join("control/run.sqlite-journal");
        let connection = Connection::open(&db).unwrap();
        if let Some(page_size) = page_size {
            connection
                .execute_batch(&format!("PRAGMA page_size={page_size}; VACUUM;"))
                .unwrap();
        }
        let original_db = fs::read(&db).unwrap();
        connection.execute_batch("PRAGMA journal_mode=DELETE; PRAGMA synchronous=FULL; PRAGMA cache_size=1; PRAGMA cache_spill=1; BEGIN IMMEDIATE;").unwrap();
        if bound_metadata {
            let original: String = connection
                .query_row(
                    "SELECT metadata_json FROM metadata WHERE singleton=1",
                    [],
                    |row| row.get(0),
                )
                .unwrap();
            // Two real SQL writes change every original UTF8 payload byte and
            // restore its exact bytes in the same transaction. Locked SQLite's
            // btreeOverwriteContent skips equal data, hence a same-value UPDATE
            // would not guarantee all overflow before-images are journaled.
            connection
                .execute(
                    "UPDATE metadata SET metadata_json=zeroblob(?1) WHERE singleton=1",
                    [original.len() as u32],
                )
                .unwrap();
            connection
                .execute(
                    "UPDATE metadata SET metadata_json=?1 WHERE singleton=1",
                    [&original],
                )
                .unwrap();
        }
        connection
            .execute_batch("UPDATE checkpoints SET payload_json=zeroblob(131072) WHERE sequence=1;")
            .unwrap();
        let dirty_db = fs::read(&db).unwrap();
        let journal_bytes = fs::read(&journal).unwrap();
        assert_eq!(
            journal_bytes.get(..8),
            Some(MAGIC.as_slice()),
            "actual spill must have a synced journal header"
        );
        assert!(be32(&journal_bytes, 8).unwrap() > 0);
        connection.execute_batch("ROLLBACK").unwrap();
        drop(connection);
        fs::write(&db, dirty_db).unwrap();
        fs::write(&journal, &journal_bytes).unwrap();
        TestRun {
            base,
            root,
            db,
            journal,
            journal_bytes,
            original_db,
        }
    }

    fn assert_preserved(run: &TestRun, passes: bool) {
        let db = fs::read(&run.db).unwrap();
        let journal = fs::read(&run.journal).unwrap();
        let owner = fs::read(run.root.join("control/owner.json")).unwrap();
        let result = validate_hot_journal(&run.db, &run.journal);
        assert_eq!(result.is_ok(), passes, "{result:?}");
        assert_eq!(fs::read(&run.db).unwrap(), db);
        assert_eq!(fs::read(&run.journal).unwrap(), journal);
        assert_eq!(
            fs::read(run.root.join("control/owner.json")).unwrap(),
            owner
        );
    }

    #[test]
    fn real_sqlite_hot_journal_is_validated_without_disk_writes() {
        let run = actual_journal();
        assert_eq!(
            checked_recovery_image(&run.db, &run.journal).unwrap(),
            run.original_db
        );
        assert_preserved(&run, true);
        // SQLite, and only SQLite, now performs recovery of the original DB.
        let connection = Connection::open(&run.db).unwrap();
        let raw: String = connection
            .query_row(
                "SELECT payload_json FROM checkpoints WHERE sequence=1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let cp: Checkpoint = serde_json::from_str(&raw).unwrap();
        assert_eq!(cp.event, "created");
        assert!(!run.journal.exists());
    }

    #[test]
    fn real_sqlite_small_page_hot_journal_is_validated() {
        let run = actual_journal_with_page_size(Some(512));
        assert_eq!(be32(&run.journal_bytes, 24).unwrap(), 512);
        assert_eq!(
            checked_recovery_image(&run.db, &run.journal).unwrap(),
            run.original_db
        );
        assert_preserved(&run, true);
    }

    #[test]
    fn locked_sqlite_sector_header_initialization_replicas_are_exact() {
        // Independently authored writeJournalHdr protocol fields: nonce=123,
        // original pages=7, sector=1024, page=512. Only syncJournal's first
        // header copy gains MAGIC and nRec=2; the second retains zero first12.
        let initialized: [u8; 28] = [
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 123, 0, 0, 0, 7, 0, 0, 4, 0, 0, 0, 2, 0,
        ];
        let mut header = vec![0; 1024];
        header[..28].copy_from_slice(&initialized);
        header[512..540].copy_from_slice(&initialized);
        header[..8].copy_from_slice(MAGIC);
        header[8..12].copy_from_slice(&2u32.to_be_bytes());
        assert!(header_padding(&header, 512, 2).is_ok());
        header[527] = 124;
        assert!(header_padding(&header, 512, 2).is_err());
    }

    #[test]
    fn cold_zero_header_and_truncated_journal_are_refused() {
        let run = actual_journal();
        let mut cold = run.journal_bytes.clone();
        cold[..12].fill(0);
        fs::write(&run.journal, cold).unwrap();
        assert_preserved(&run, false);
        fs::write(
            &run.journal,
            &run.journal_bytes[..run.journal_bytes.len() - 1],
        )
        .unwrap();
        assert_preserved(&run, false);
    }

    #[test]
    fn corrupt_checksum_page_number_sector_and_padding_are_refused() {
        let run = actual_journal();
        let sector = be32(&run.journal_bytes, 20).unwrap() as usize;
        let page_size = be32(&run.journal_bytes, 24).unwrap() as usize;
        for offset in [sector + 4 + page_size - 200, sector, 20, 28] {
            let mut damaged = run.journal_bytes.clone();
            damaged[offset] ^= 0x80;
            fs::write(&run.journal, damaged).unwrap();
            assert_preserved(&run, false);
        }
    }

    #[test]
    fn valid_foreign_run_journal_is_refused_before_real_recovery() {
        let run = actual_journal();
        let foreign = actual_journal();
        fs::write(&run.journal, &foreign.journal_bytes).unwrap();
        assert_preserved(&run, false);
    }

    #[test]
    fn real_shared_page_only_journal_cannot_borrow_unchanged_run_identity() {
        // This is the original failed foreign-test generation protocol, kept
        // as an explicit refusal. Its synced segment may contain only page1;
        // the identifying checkpoint page is in an ignored zero-magic tail.
        let run = actual_journal_with_options(None, false);
        assert_preserved(&run, false);
    }

    #[test]
    fn recovery_witness_detects_bytes_and_replaced_journal_identity() {
        let run = actual_journal();
        let witness = RecoveryWitness::capture(&run.db, &run.journal).unwrap();
        witness.verify(&run.db, &run.journal).unwrap();
        let mut changed = run.journal_bytes.clone();
        changed[12] ^= 1;
        fs::write(&run.journal, changed).unwrap();
        assert!(witness.verify(&run.db, &run.journal).is_err());
        fs::write(&run.journal, &run.journal_bytes).unwrap();
        let witness = RecoveryWitness::capture(&run.db, &run.journal).unwrap();
        let original = run.base.join("original-owned-journal");
        fs::rename(&run.journal, &original).unwrap();
        fs::write(&run.journal, &run.journal_bytes).unwrap();
        assert!(witness.verify(&run.db, &run.journal).is_err());
    }

    #[test]
    fn actual_selected_input_change_blocks_hot_journal_recovery() {
        let run = actual_journal();
        let owner: Owner =
            serde_json::from_slice(&fs::read(run.root.join("control/owner.json")).unwrap())
                .unwrap();
        let case_path = Path::new(&owner.case_root).join("case.json");
        let mut bytes = fs::read(&case_path).unwrap();
        bytes.push(b'\n');
        fs::write(&case_path, bytes).unwrap();
        assert_preserved(&run, false);
    }

    #[test]
    fn journal_and_database_hard_links_are_refused() {
        let run = actual_journal();
        let link = run.base.join("journal-link");
        fs::hard_link(&run.journal, &link).unwrap();
        assert_preserved(&run, false);
        fs::remove_file(link).unwrap();
        fs::hard_link(&run.db, run.base.join("db-link")).unwrap();
        assert_preserved(&run, false);
    }

    #[test]
    fn wrong_journal_path_is_refused() {
        let run = actual_journal();
        let other = run.root.join("control/foreign-journal");
        fs::write(&other, &run.journal_bytes).unwrap();
        assert!(validate_hot_journal(&run.db, &other).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn journal_symlink_is_refused() {
        let run = actual_journal();
        let original = run.base.join("actual-journal");
        fs::rename(&run.journal, &original).unwrap();
        std::os::unix::fs::symlink(&original, &run.journal).unwrap();
        assert_preserved(&run, false);
    }
}
