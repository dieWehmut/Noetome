//! Source-backed context reconstruction and Gregorian temporal ranges.
use crate::{
    knowledge_coordinate_input::{VerifiedReview, err, plain},
    knowledge_coordination_v2_model::{AtomContext, Context, Time},
    project::ProjectError,
    text::sha256,
};
use std::collections::BTreeSet;
fn bad() -> ProjectError {
    err(
        "COORD_CONTEXT",
        3,
        "invalid, incomplete, nonadjacent or unsupported source context",
    )
}
pub(crate) fn range(t: &Time) -> Result<Option<(u32, u32)>, ProjectError> {
    if t.status == "unknown" {
        return if t.precision.is_none() && t.value.is_none() {
            Ok(None)
        } else {
            Err(bad())
        };
    }
    let v = t.value.as_deref().ok_or_else(bad)?;
    let year = |s: &str| -> Result<u32, ProjectError> {
        if s.len() != 4 || !s.bytes().all(|b| b.is_ascii_digit()) {
            return Err(bad());
        }
        let n = s.parse::<u32>().map_err(|_| bad())?;
        if n == 0 { Err(bad()) } else { Ok(n) }
    };
    match t.precision.as_deref() {
        Some("year") => {
            let y = year(v)?;
            Ok(Some((y * 10000 + 101, y * 10000 + 1231)))
        }
        Some("day") => {
            if v.len() != 10
                || v.as_bytes()[4] != b'-'
                || v.as_bytes()[7] != b'-'
                || !v
                    .bytes()
                    .enumerate()
                    .all(|(i, b)| i == 4 || i == 7 || b.is_ascii_digit())
            {
                return Err(bad());
            }
            let y = year(&v[..4])?;
            let m = v[5..7].parse::<u32>().map_err(|_| bad())?;
            let d = v[8..].parse::<u32>().map_err(|_| bad())?;
            let days = match m {
                1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
                4 | 6 | 9 | 11 => 30,
                2 => {
                    if y % 4 == 0 && (y % 100 != 0 || y % 400 == 0) {
                        29
                    } else {
                        28
                    }
                }
                _ => return Err(bad()),
            };
            if d == 0 || d > days {
                return Err(bad());
            }
            let n = y * 10000 + m * 100 + d;
            Ok(Some((n, n)))
        }
        _ => Err(bad()),
    }
}
pub(crate) fn order(a: &Time, b: &Time) -> Result<Option<bool>, ProjectError> {
    match (range(a)?, range(b)?) {
        (Some(a), Some(b)) if a.1 < b.0 => Ok(Some(true)),
        (Some(a), Some(b)) if b.1 < a.0 => Ok(Some(false)),
        _ => Ok(None),
    }
}
pub(crate) fn rebuild(r: &VerifiedReview, c: &AtomContext) -> Result<Context, ProjectError> {
    let u = &r.source.units[0];
    let text = &u.text;
    let gold = &r.gold.quote;
    let matches = text.match_indices(gold).collect::<Vec<_>>();
    if gold.is_empty() || matches.len() != 1 {
        return Err(bad());
    }
    let fact_start = matches[0].0;
    if fact_start == 0 || text.as_bytes()[fact_start - 1] != b'\n' {
        return Err(bad());
    }
    let line_end = fact_start - 1;
    let start = text[..line_end].rfind('\n').map_or(0, |n| n + 1);
    let line = &text[start..line_end];
    let (status, time) = line
        .strip_prefix("适用条件：")
        .and_then(|s| s.strip_suffix('。'))
        .and_then(|s| s.split_once("；观点时间："))
        .ok_or_else(bad)?;
    let applicability = match status {
        "已列明" => "known",
        "未给出" => "unknown",
        "不适用" => "not_applicable",
        _ => return Err(bad()),
    };
    let actual = if time == "未给出" {
        Time {
            status: "unknown".into(),
            precision: None,
            value: None,
        }
    } else if let Some(y) = time.strip_suffix('年') {
        Time {
            status: "known".into(),
            precision: Some("year".into()),
            value: Some(y.into()),
        }
    } else {
        Time {
            status: "known".into(),
            precision: Some("day".into()),
            value: Some(time.into()),
        }
    };
    range(&actual)?;
    range(&c.time)?;
    let conditions_match = if applicability == "known" {
        !r.gold.conditions.is_empty()
    } else {
        r.gold.conditions.is_empty()
    };
    if applicability != c.applicability_status
        || actual.status != c.time.status
        || actual.precision != c.time.precision
        || actual.value != c.time.value
        || !conditions_match
    {
        return Err(bad());
    }
    let end = fact_start + gold.len();
    // Candidate evidence and emitted context both use absolute source byte offsets.
    let absolute_start = u.start_byte + start;
    let absolute_end = u.start_byte + end;
    let covered = r.candidate.payload.atoms[0]
        .atom
        .evidence_links
        .iter()
        .filter(|l| l.relation == crate::fixture_model::EvidenceRelation::Supports)
        .filter_map(|l| {
            r.candidate
                .payload
                .evidence
                .iter()
                .find(|e| {
                    e.evidence_id == l.evidence_id
                        && e.unit_id == u.unit_id
                        && e.excerpt_start_byte <= absolute_start
                        && e.excerpt_end_byte >= absolute_end
                })
                .map(|e| e.evidence_id.clone())
        })
        .collect::<BTreeSet<_>>();
    let declared = c
        .context_evidence_ids
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    if declared.is_empty()
        || c.context_evidence_ids.len() > 1024
        || declared.len() != c.context_evidence_ids.len()
        || c.context_evidence_ids.iter().any(|s| !plain(s))
        || declared != covered
    {
        return Err(bad());
    }
    let quote = text[start..end].to_owned();
    Ok(Context {
        applicability_status: applicability.into(),
        time: actual,
        evidence_ids: declared.into_iter().collect(),
        unit_id: u.unit_id.clone(),
        start_byte: absolute_start,
        end_byte: absolute_end,
        sha256: sha256(quote.as_bytes()),
        quote,
    })
}
