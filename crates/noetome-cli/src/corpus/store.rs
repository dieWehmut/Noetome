use super::*;
use rusqlite::{Connection, OptionalExtension, params};
use std::{fs::File, path::PathBuf, time::Duration};

const SCHEMA: &str = "
CREATE TABLE metadata(singleton INTEGER PRIMARY KEY CHECK(singleton=1), project_id TEXT NOT NULL, schema_version INTEGER NOT NULL CHECK(schema_version=1));
CREATE TABLE sources(source_id TEXT PRIMARY KEY,display_name TEXT NOT NULL,selected_path TEXT NOT NULL,content_sha256 TEXT UNIQUE,byte_length INTEGER NOT NULL,status TEXT NOT NULL,page_count INTEGER,created_at TEXT NOT NULL,pause_requested INTEGER NOT NULL DEFAULT 0,error_code TEXT,error_message TEXT,preflight_json TEXT);
CREATE TABLE pages(source_id TEXT NOT NULL REFERENCES sources(source_id),page_number INTEGER NOT NULL CHECK(page_number>0),status TEXT NOT NULL DEFAULT 'pending',page_json TEXT,page_sha256 TEXT,PRIMARY KEY(source_id,page_number));
CREATE INDEX page_status ON pages(source_id,status,page_number);
PRAGMA application_id=1313817424;
PRAGMA user_version=1;
";
pub(super) struct Repository {
    pub root: PathBuf,
    connection: Connection,
}
impl Repository {
    fn open(project_path: &Path, create: bool) -> Result<Option<Self>, ProjectError> {
        let info = project::inspect(project_path)?;
        let project_path = project::check_components(project_path)?;
        crate::extraction_preview::ensure_outside_vault(&project_path)?;
        let root = project_path.join(".noetome-corpus");
        project::check_components(&root)?;
        if !root.exists() {
            if !create {
                return Ok(None);
            }
            if let Err(e) = fs::create_dir(&root) {
                if e.kind() != std::io::ErrorKind::AlreadyExists {
                    return Err(io(&root, e));
                }
            }
        }
        let init_path = root.join("initialize.lock");
        project::check_components(&init_path)?;
        let init = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&init_path)
            .map_err(|e| io(&init_path, e))?;
        init.lock().map_err(|e| io(&init_path, e))?;
        let path = root.join("corpus.sqlite");
        project::check_components(&path)?;
        let exists = path.exists();
        if !exists && !create {
            return Err(error(
                "CORPUS_INCOMPLETE",
                "资料库目录存在但数据库缺失，已保留现场。",
            ));
        }
        // WAL belongs to this mutable repository; the legacy project database
        // remains untouched and keeps its read-only snapshot contract.
        for suffix in ["-wal", "-shm", "-journal"] {
            project::check_components(&root.join(format!("corpus.sqlite{suffix}")))?;
        }
        let connection = Connection::open(&path).map_err(ProjectError::database)?;
        connection
            .busy_timeout(Duration::from_secs(5))
            .map_err(ProjectError::database)?;
        connection
            .execute_batch(
                "PRAGMA foreign_keys=ON; PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL;",
            )
            .map_err(ProjectError::database)?;
        if !exists {
            let tx = connection
                .unchecked_transaction()
                .map_err(ProjectError::database)?;
            tx.execute_batch(SCHEMA).map_err(ProjectError::database)?;
            tx.execute("INSERT INTO metadata VALUES(1,?1,1)", [&info.project_id])
                .map_err(ProjectError::database)?;
            tx.commit().map_err(ProjectError::database)?;
        }
        let metadata: (String, u32) = connection
            .query_row(
                "SELECT project_id,schema_version FROM metadata WHERE singleton=1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .map_err(ProjectError::database)?;
        if metadata != (info.project_id, 1) {
            return Err(error("CORPUS_SCHEMA", "资料库版本或项目身份不匹配。"));
        }
        connection.execute_batch("CREATE TABLE IF NOT EXISTS page_revisions(source_id TEXT NOT NULL,page_number INTEGER NOT NULL,page_sha256 TEXT NOT NULL,page_json TEXT NOT NULL,PRIMARY KEY(source_id,page_number,page_sha256));").map_err(ProjectError::database)?;
        Ok(Some(Self { root, connection }))
    }
    pub fn create(path: &Path) -> Result<Self, ProjectError> {
        Self::open(path, true)?.ok_or_else(|| error("CORPUS_MISSING", "资料库不存在。"))
    }
    pub fn open_existing(path: &Path) -> Result<Option<Self>, ProjectError> {
        Self::open(path, false)
    }
    pub fn required(path: &Path) -> Result<Self, ProjectError> {
        Self::open(path, false)?.ok_or_else(|| error("CORPUS_MISSING", "请先导入PDF。"))
    }
    pub fn source_directory(&self, id: &str) -> Result<PathBuf, ProjectError> {
        crate::source::invalid_id(id)?;
        project::check_components(&self.root.join(id))
    }
    pub fn lease(&self, id: &str) -> Result<File, ProjectError> {
        let path = self.source_directory(id)?.join("worker.lock");
        project::check_components(&path)?;
        let file = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)
            .map_err(|e| io(&path, e))?;
        file.try_lock().map_err(|_| {
            error(
                "CORPUS_BUSY",
                "这本书正在另一个进程中处理；可以查看进度或暂停。",
            )
        })?;
        Ok(file)
    }
    fn acquisition_active(&self, id: &str) -> Result<bool, ProjectError> {
        let path = self.source_directory(id)?.join("worker.lock");
        project::check_components(&path)?;
        if !path.exists() {
            return Ok(false);
        }
        let file = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&path)
            .map_err(|e| io(&path, e))?;
        match file.try_lock() {
            Ok(()) => Ok(false),
            Err(fs::TryLockError::WouldBlock) => Ok(true),
            Err(fs::TryLockError::Error(e)) => Err(io(&path, e)),
        }
    }
    pub fn insert_source(&self, id: &str, path: &Path, length: u64) -> Result<(), ProjectError> {
        let now = time::OffsetDateTime::now_utc()
            .format(&time::format_description::well_known::Rfc3339)
            .map_err(|e| error("CORPUS_TIME", e.to_string()))?;
        self.connection.execute("INSERT INTO sources(source_id,display_name,selected_path,byte_length,status,created_at) VALUES(?1,?2,?3,?4,'acquiring',?5)",params![id,path.file_name().unwrap_or_default().to_string_lossy(),path.to_string_lossy(),crate::source::sql_integer(length)?,now]).map_err(ProjectError::database)?;
        Ok(())
    }
    pub fn acquired(&self, id: &str, hash: &str, length: u64) -> Result<(), ProjectError> {
        self.connection.execute("UPDATE sources SET content_sha256=?2,byte_length=?3,status='queued' WHERE source_id=?1",params![id,hash,crate::source::sql_integer(length)?]).map_err(ProjectError::database)?;
        Ok(())
    }
    pub fn find_hash(&self, hash: &str) -> Result<Option<String>, ProjectError> {
        self.connection
            .query_row(
                "SELECT source_id FROM sources WHERE content_sha256=?1",
                [hash],
                |r| r.get(0),
            )
            .optional()
            .map_err(ProjectError::database)
    }
    pub fn delete_acquiring(&self, id: &str) -> Result<(), ProjectError> {
        self.connection
            .execute(
                "DELETE FROM sources WHERE source_id=?1 AND status='acquiring'",
                [id],
            )
            .map_err(ProjectError::database)?;
        Ok(())
    }
    pub fn fail(&self, id: &str, code: &str, message: &str) -> Result<(), ProjectError> {
        self.connection.execute("UPDATE sources SET status='failed',error_code=?2,error_message=?3 WHERE source_id=?1",params![id,code,message]).map_err(ProjectError::database)?;
        Ok(())
    }
    pub fn register_pages(&self, id: &str, flight: &Value) -> Result<(), ProjectError> {
        let count = flight
            .get("page_count")
            .and_then(Value::as_u64)
            .filter(|v| *v > 0 && *v <= u32::MAX as u64);
        let tx = self
            .connection
            .unchecked_transaction()
            .map_err(ProjectError::database)?;
        if flight.get("status").and_then(Value::as_str) != Some("ready") || count.is_none() {
            let code = if flight["password_required"] == true {
                "PDF_PASSWORD_REQUIRED"
            } else {
                "PDF_PREFLIGHT_FAILED"
            };
            tx.execute("UPDATE sources SET status='failed',preflight_json=?2,error_code=?3,error_message=?4 WHERE source_id=?1",params![id,flight.to_string(),code,flight["issues"].to_string()]).map_err(ProjectError::database)?;
        } else {
            let count = count.unwrap() as u32;
            tx.execute("UPDATE sources SET page_count=?2,status='queued',preflight_json=?3,error_code=NULL,error_message=NULL WHERE source_id=?1",params![id,count,flight.to_string()]).map_err(ProjectError::database)?;
            {
                let mut insert = tx
                    .prepare("INSERT OR IGNORE INTO pages(source_id,page_number) VALUES(?1,?2)")
                    .map_err(ProjectError::database)?;
                for page in 1..=count {
                    insert
                        .execute(params![id, page])
                        .map_err(ProjectError::database)?;
                }
            }
        }
        tx.commit().map_err(ProjectError::database)
    }
    pub fn summary(&self, id: &str) -> Result<SourceSummary, ProjectError> {
        crate::source::invalid_id(id)?;
        let mut summary=self.connection.query_row("SELECT source_id,display_name,content_sha256,byte_length,status,page_count,created_at,error_code,error_message FROM sources WHERE source_id=?1",[id],|r|{
            let code:Option<String>=r.get(7)?;
            Ok(SourceSummary{source_id:r.get(0)?,display_name:r.get(1)?,content_sha256:r.get(2)?,byte_length:r.get::<_,i64>(3)? as u64,status:r.get(4)?,page_count:r.get(5)?,created_at:r.get(6)?,error:code.map(|code|Failure{code,message:r.get(8).unwrap_or_default()}),processed_pages:0,text_pages:0,ocr_pages:0,needs_ocr_pages:0,review_pages:0,failed_pages:0,blank_pages:0})
        }).optional().map_err(ProjectError::database)?.ok_or_else(||error("CORPUS_NOT_FOUND","资料身份不在当前项目中。"))?;
        let mut query = self
            .connection
            .prepare("SELECT status,count(*) FROM pages WHERE source_id=?1 GROUP BY status")
            .map_err(ProjectError::database)?;
        let rows = query
            .query_map([id], |r| Ok((r.get::<_, String>(0)?, r.get::<_, u32>(1)?)))
            .map_err(ProjectError::database)?;
        for row in rows {
            let (state, count) = row.map_err(ProjectError::database)?;
            if state != "pending" {
                summary.processed_pages += count;
            }
            match state.as_str() {
                "text_ready" => summary.text_pages += count,
                "ocr_ready" => summary.ocr_pages += count,
                "needs_ocr" => {
                    summary.needs_ocr_pages += count;
                    summary.review_pages += count;
                }
                "needs_review" => summary.review_pages += count,
                "failed" => summary.failed_pages += count,
                "blank" => summary.blank_pages += count,
                _ => {}
            }
        }
        // Text availability, OCR provenance, and review status are independent
        // dimensions: a recognized table can contain text and still need review.
        let available:(u32,u32)=self.connection.query_row("SELECT coalesce(sum(CASE WHEN status!='failed' AND length(trim(json_extract(page_json,'$.text')))>0 THEN 1 ELSE 0 END),0),coalesce(sum(CASE WHEN status!='failed' AND json_extract(page_json,'$.parser.engine') IN ('mineru','paddleocr') THEN 1 ELSE 0 END),0) FROM pages WHERE source_id=?1",[id],|r|Ok((r.get(0)?,r.get(1)?))).map_err(ProjectError::database)?;
        summary.text_pages = available.0;
        summary.ocr_pages = available.1;
        if summary.status == "acquiring" && !self.acquisition_active(id)? {
            // A stale lock file is not an active process. This read projection
            // preserves the acquisition row and partial bytes for diagnosis.
            summary.status = "interrupted".into();
            summary.error = Some(Failure {
                code: "CORPUS_ACQUISITION_INTERRUPTED".into(),
                message: "获取原件的进程已经退出；部分原件尚未确认，请重新导入。".into(),
            });
        }
        Ok(summary)
    }
    pub fn list(&self) -> Result<Vec<SourceSummary>, ProjectError> {
        let mut q = self
            .connection
            .prepare("SELECT source_id FROM sources ORDER BY created_at DESC,source_id")
            .map_err(ProjectError::database)?;
        let ids = q
            .query_map([], |r| r.get::<_, String>(0))
            .map_err(ProjectError::database)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(ProjectError::database)?;
        ids.iter().map(|id| self.summary(id)).collect()
    }
    pub fn pages(&self, id: &str, offset: u32, limit: u32) -> Result<Vec<Value>, ProjectError> {
        let mut q=self.connection.prepare("SELECT page_number,status,page_json,page_sha256 FROM pages WHERE source_id=?1 AND page_number>?2 ORDER BY page_number LIMIT ?3").map_err(ProjectError::database)?;
        let rows = q
            .query_map(params![id, offset, limit], |r| {
                Ok((
                    r.get::<_, u32>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, Option<String>>(2)?,
                    r.get::<_, Option<String>>(3)?,
                ))
            })
            .map_err(ProjectError::database)?;
        let mut pages = vec![];
        for row in rows {
            let (number, status, raw, hash) = row.map_err(ProjectError::database)?;
            let page = if let Some(raw) = raw {
                if Some(crate::text::sha256(raw.as_bytes())) != hash {
                    return Err(error("CORPUS_PAGE_CHANGED", "保存页的字节摘要不匹配。"));
                }
                serde_json::from_str(&raw)
                    .map_err(|e| error("CORPUS_PAGE_INVALID", e.to_string()))?
            } else {
                json!({"page_number":number,"status":status,"text":"","regions":[],"issues":[]})
            };
            self.assets(id, &page)?;
            pages.push(page);
        }
        Ok(pages)
    }
    pub fn request_pause(&self, id: &str) -> Result<(), ProjectError> {
        self.summary(id)?;
        self.connection
            .execute(
                "UPDATE sources SET pause_requested=1 WHERE source_id=?1",
                [id],
            )
            .map_err(ProjectError::database)?;
        Ok(())
    }
    pub fn paused(&self, id: &str) -> Result<bool, ProjectError> {
        self.connection
            .query_row(
                "SELECT pause_requested FROM sources WHERE source_id=?1",
                [id],
                |r| r.get(0),
            )
            .map_err(ProjectError::database)
    }
    pub fn start(&self, id: &str) -> Result<(), ProjectError> {
        self.connection.execute("UPDATE sources SET status='running',pause_requested=0,error_code=NULL,error_message=NULL WHERE source_id=?1",[id]).map_err(ProjectError::database)?;
        Ok(())
    }
    pub fn next_batch(&self, id: &str, batch: u32) -> Result<Option<(u32, u32)>, ProjectError> {
        let first: Option<u32> = self
            .connection
            .query_row(
                "SELECT min(page_number) FROM pages WHERE source_id=?1 AND status='pending'",
                [id],
                |r| r.get(0),
            )
            .map_err(ProjectError::database)?;
        let Some(first) = first else { return Ok(None) };
        let mut end = self
            .summary(id)?
            .page_count
            .unwrap_or(first)
            .min(first.saturating_add(batch - 1));
        let confirmed:Option<u32> = self.connection.query_row("SELECT min(page_number) FROM pages WHERE source_id=?1 AND page_number>?2 AND page_number<=?3 AND status!='pending'",params![id,first,end],|r|r.get(0)).map_err(ProjectError::database)?;
        if let Some(confirmed) = confirmed {
            end = confirmed - 1;
        }
        Ok(Some((first, end)))
    }
    pub fn commit_page(&self, id: &str, mut page: Value) -> Result<(), ProjectError> {
        self.store_page(id, &mut page, "pending")
    }
    pub fn commit_ocr_page(&self, id: &str, mut page: Value) -> Result<(), ProjectError> {
        self.store_page(id, &mut page, "needs_ocr")
    }
    pub fn next_ocr(&self, id: &str) -> Result<Option<Value>, ProjectError> {
        let number: Option<u32> = self
            .connection
            .query_row(
                "SELECT min(page_number) FROM pages WHERE source_id=?1 AND status='needs_ocr'",
                [id],
                |r| r.get(0),
            )
            .map_err(ProjectError::database)?;
        match number {
            Some(n) => self.pages(id, n - 1, 1).map(|mut p| p.pop()),
            None => Ok(None),
        }
    }
    pub fn retry_failed(&self, id: &str) -> Result<(), ProjectError> {
        let tx = self
            .connection
            .unchecked_transaction()
            .map_err(ProjectError::database)?;
        let mut query = tx.prepare("SELECT page_number,page_json,page_sha256 FROM pages WHERE source_id=?1 AND status='failed'").map_err(ProjectError::database)?;
        let failures = query
            .query_map([id], |row| {
                Ok((
                    row.get::<_, u32>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })
            .map_err(ProjectError::database)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(ProjectError::database)?;
        drop(query);
        for (number, raw, hash) in failures {
            if crate::text::sha256(raw.as_bytes()) != hash {
                return Err(error(
                    "CORPUS_PAGE_CHANGED",
                    "失败页的字节摘要不匹配，不能重试。",
                ));
            }
            tx.execute(
                "INSERT OR IGNORE INTO page_revisions VALUES(?1,?2,?3,?4)",
                params![id, number, hash, raw],
            )
            .map_err(ProjectError::database)?;
            let page: Value = serde_json::from_str(&raw)
                .map_err(|e| error("CORPUS_PAGE_INVALID", e.to_string()))?;
            if matches!(
                page["parser"]["engine"].as_str(),
                Some("mineru" | "paddleocr")
            ) {
                // The original text-layer result is immutable. Restore it only
                // as the current pending OCR input; retain the failed attempt.
                let original:Option<(String,String)> = tx.query_row("SELECT page_json,page_sha256 FROM page_revisions WHERE source_id=?1 AND page_number=?2 AND json_extract(page_json,'$.status')='needs_ocr' AND json_extract(page_json,'$.parser.engine')='pdfplumber'",params![id,number],|row|Ok((row.get(0)?,row.get(1)?))).optional().map_err(ProjectError::database)?;
                let (original, hash) = original.ok_or_else(|| {
                    error(
                        "CORPUS_OCR_ORIGINAL",
                        "OCR失败页缺少原始页版本，不能重新识别。",
                    )
                })?;
                if crate::text::sha256(original.as_bytes()) != hash {
                    return Err(error("CORPUS_PAGE_CHANGED", "原始页版本的字节摘要不匹配。"));
                }
                tx.execute("UPDATE pages SET status='needs_ocr',page_json=?3,page_sha256=?4 WHERE source_id=?1 AND page_number=?2",params![id,number,original,hash]).map_err(ProjectError::database)?;
            } else {
                tx.execute("UPDATE pages SET status='pending',page_json=NULL,page_sha256=NULL WHERE source_id=?1 AND page_number=?2",params![id,number]).map_err(ProjectError::database)?;
            }
        }
        tx.commit().map_err(ProjectError::database)
    }
    fn store_page(
        &self,
        id: &str,
        page: &mut Value,
        expected_status: &str,
    ) -> Result<(), ProjectError> {
        self.prepare_assets(id, page)?;
        let number = page["page_number"]
            .as_u64()
            .filter(|n| *n > 0 && *n <= u32::MAX as u64)
            .ok_or_else(|| error("CORPUS_PAGE_INVALID", "物理页号无效。"))?;
        let state = page["status"].as_str().unwrap_or_default().to_owned();
        if ![
            "text_ready",
            "ocr_ready",
            "needs_ocr",
            "needs_review",
            "failed",
            "blank",
        ]
        .contains(&state.as_str())
            || page["schema_version"] != 1
        {
            return Err(error(
                "CORPUS_PAGE_INVALID",
                "解析器返回了不支持的页状态或协议。",
            ));
        }
        let text = page["text"]
            .as_str()
            .ok_or_else(|| error("CORPUS_PAGE_INVALID", "页面缺少文本字段。"))?;
        if matches!(state.as_str(), "text_ready" | "ocr_ready") && text.trim().is_empty() {
            return Err(error("CORPUS_PAGE_INVALID", "就绪页不能没有文本。"));
        }
        let width = page["width_pt"].as_f64().unwrap_or(0.0);
        let height = page["height_pt"].as_f64().unwrap_or(0.0);
        if state != "failed" && (width <= 0.0 || height <= 0.0 || !page["geometry"].is_object()) {
            return Err(error(
                "CORPUS_PAGE_INVALID",
                "非失败页必须有有效页面尺寸和坐标变换。",
            ));
        }
        if !page["regions"].is_array() {
            return Err(error("CORPUS_PAGE_INVALID", "页面缺少区域清单。"));
        }
        let identity=crate::text::sha256(json!({"source_id":id,"page_number":number,"geometry":page["geometry"],"text":page["text"],"regions":page["regions"],"status":page["status"],"issues":page["issues"],"parser":page["parser"]}).to_string().as_bytes());
        let regions = page["regions"]
            .as_array_mut()
            .expect("validated region array");
        for (ordinal, region) in regions.iter_mut().enumerate() {
            let bbox = region["bbox"]
                .as_array()
                .filter(|b| b.len() == 4)
                .ok_or_else(|| error("CORPUS_PAGE_INVALID", "区域缺少四元坐标。"))?;
            let b = bbox
                .iter()
                .map(|v| v.as_f64().filter(|x| x.is_finite()))
                .collect::<Option<Vec<_>>>()
                .ok_or_else(|| error("CORPUS_PAGE_INVALID", "区域坐标必须为有限数字。"))?;
            if width <= 0.0
                || height <= 0.0
                || b[0] < -0.01
                || b[1] < -0.01
                || b[2] < b[0]
                || b[3] < b[1]
                || b[2] > width + 0.01
                || b[3] > height + 0.01
                || region["text"].as_str().is_none()
            {
                return Err(error("CORPUS_PAGE_INVALID", "区域坐标越界或文本无效。"));
            }
            if region["ordinal"].as_u64() != Some(ordinal as u64) {
                return Err(error("CORPUS_PAGE_INVALID", "区域顺序编号不连续。"));
            }
            region["region_id"] = json!(format!("{id}:p{number}:{}:r{ordinal}", &identity[..16]));
        }
        page["source_id"] = json!(id);
        page["page_revision"] = json!(identity);
        let raw = page.to_string();
        if raw.len() as u64 > MAX_PAGE_BYTES {
            return Err(error(
                "CORPUS_PAGE_LIMIT",
                "单页结构超过16MiB；该页保留待处理，不截断文本。",
            ));
        }
        let tx = self
            .connection
            .unchecked_transaction()
            .map_err(ProjectError::database)?;
        tx.execute("INSERT OR IGNORE INTO page_revisions SELECT source_id,page_number,page_sha256,page_json FROM pages WHERE source_id=?1 AND page_number=?2 AND status=?3 AND page_json IS NOT NULL",params![id,number as u32,expected_status]).map_err(ProjectError::database)?;
        let changed=tx.execute("UPDATE pages SET status=?3,page_json=?4,page_sha256=?5 WHERE source_id=?1 AND page_number=?2 AND status=?6",params![id,number as u32,state,raw,crate::text::sha256(raw.as_bytes()),expected_status]).map_err(ProjectError::database)?;
        if changed != 1 {
            return Err(error(
                "CORPUS_PAGE_CONFLICT",
                "页号不在待处理范围或已经确认。",
            ));
        }
        tx.execute("INSERT OR IGNORE INTO page_revisions(source_id,page_number,page_sha256,page_json) VALUES(?1,?2,?3,?4)",params![id,number as u32,crate::text::sha256(raw.as_bytes()),raw]).map_err(ProjectError::database)?;
        tx.commit().map_err(ProjectError::database)?;
        Ok(())
    }
    pub fn finish(&self, id: &str) -> Result<(), ProjectError> {
        let summary = self.summary(id)?;
        let state = if self.paused(id)? || summary.processed_pages < summary.page_count.unwrap_or(0)
        {
            "paused"
        } else if summary.failed_pages > 0 || summary.review_pages > 0 {
            "needs_review"
        } else {
            "ready"
        };
        self.connection
            .execute(
                "UPDATE sources SET status=?2 WHERE source_id=?1",
                params![id, state],
            )
            .map_err(ProjectError::database)?;
        Ok(())
    }
}
