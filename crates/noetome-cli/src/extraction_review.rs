//! Deterministic, restricted Chinese measurement grammar; never repairs inputs.
use super::extraction_review_model::*;
use crate::{
    extraction_model::RequestUnit,
    fixture_model::{EvidenceRelation, ValueStatus},
    project::ProjectError,
};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
pub(crate) fn normalized(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).collect()
}
fn plain(s: &str) -> bool {
    !s.trim().is_empty() && s.len() <= 1024 && crate::fixture_plan::plain_text(s)
}
pub(crate) fn set_ok(items: &[String]) -> bool {
    items.len() <= 8
        && items.iter().all(|s| plain(s))
        && items
            .iter()
            .map(|s| normalized(s))
            .collect::<BTreeSet<_>>()
            .len()
            == items.len()
}
fn split_items(s: &str) -> Vec<String> {
    s.replace("，且", "，")
        .replace(['、'], "，")
        .replace("且", "，")
        .split('，')
        .map(str::to_owned)
        .collect()
}
fn same_set(a: &[String], b: &[String]) -> bool {
    a.len() == b.len()
        && a.iter().map(|s| normalized(s)).collect::<BTreeSet<_>>()
            == b.iter().map(|s| normalized(s)).collect::<BTreeSet<_>>()
}
struct Body {
    conditions: Vec<String>,
    exceptions: Vec<String>,
}
fn body(text: &str, r: &Review, g: &Gold) -> Option<Body> {
    body_impl(text, r, g, false)
}
fn body_impl(text: &str, r: &Review, g: &Gold, source_quote: bool) -> Option<Body> {
    let n = normalized(text);
    let n = n.strip_suffix('。').unwrap_or(&n);
    let fact = if g.unknown {
        format!(
            "{}的{}未知",
            normalized(&r.subject),
            normalized(&r.property)
        )
    } else {
        format!(
            "{}的{}为{}{}",
            normalized(&r.subject),
            normalized(&r.property),
            normalized(g.value.as_deref()?),
            normalized(g.unit.as_deref()?)
        )
    };
    let mut roots = vec![fact];
    if !g.unknown {
        for ctx in &r.allowed_contexts {
            roots.push(format!(
                "{}，{}为{}{}",
                normalized(ctx),
                normalized(&r.property),
                normalized(g.value.as_deref()?),
                normalized(g.unit.as_deref()?)
            ));
        }
    } else if source_quote {
        for context in &r.allowed_contexts {
            roots.push(format!(
                "{}，{}未知",
                normalized(context),
                normalized(&r.property)
            ));
        }
    }
    let tail = roots.iter().find_map(|root| n.strip_prefix(root))?;
    let (tail, ex) = if let Some((a, b)) = tail.split_once("；例外为") {
        (a, split_items(b))
    } else {
        (tail, vec![])
    };
    let conditions = if tail.is_empty() {
        vec![]
    } else {
        let tail = tail.strip_prefix([',', '，', ';', '；', '。'])?;
        let tail = tail
            .strip_prefix("条件为")
            .or_else(|| tail.strip_prefix("条件是"))?;
        split_items(tail)
    };
    Some(Body {
        conditions,
        exceptions: ex,
    })
}
pub(crate) fn validate_gold(r: &Review, g: &Gold, u: &RequestUnit) -> Result<(), ProjectError> {
    let fail = || {
        ProjectError::new(
            "VERIFY_GOLD",
            2,
            "原gold未通过实际来源、受控正文或必要字段核对；不能用于评分。",
        )
    };
    if !plain(&r.subject)
        || !plain(&r.property)
        || !plain(&g.concept_name)
        || normalized(&g.concept_name)
            != format!("{}{}", normalized(&r.subject), normalized(&r.property))
        || !set_ok(&g.conditions)
        || !set_ok(&g.exceptions)
        || !set_ok(&r.allowed_contexts)
        || g.quote.is_empty()
        || !u.text.contains(&g.quote)
    {
        return Err(fail());
    }
    if g.unknown {
        if g.value.is_some() {
            return Err(fail());
        }
    } else if !g.value.as_deref().is_some_and(plain) || !g.unit.as_deref().is_some_and(plain) {
        return Err(fail());
    }
    let q = normalized(&g.quote);
    for required in std::iter::once(&r.subject)
        .chain(std::iter::once(&r.property))
        .chain(g.value.iter())
        .chain(g.unit.iter())
        .chain(g.conditions.iter())
        .chain(g.exceptions.iter())
    {
        if !q.contains(&normalized(required)) {
            return Err(fail());
        }
    }
    for c in &r.allowed_contexts {
        if normalized(c) != format!("在本夹具的{}中", normalized(&r.subject))
            || !g.quote.contains(c)
        {
            return Err(fail());
        }
    }
    // Token presence cannot bind a value: 10 is also present in 110.
    let source_fact = body_impl(&g.quote, r, g, true).ok_or_else(fail)?;
    if !same_set(&source_fact.conditions, &g.conditions)
        || !same_set(&source_fact.exceptions, &g.exceptions)
    {
        return Err(fail());
    }
    let b = body(&g.content, r, g).ok_or_else(fail)?;
    if !same_set(&b.conditions, &g.conditions) || !same_set(&b.exceptions, &g.exceptions) {
        return Err(fail());
    }
    Ok(())
}
pub(crate) struct Assessment {
    pub(crate) checks: Vec<Check>,
    pub(crate) expected: Vec<ObjectResult>,
    pub(crate) candidates: Vec<ObjectResult>,
    pub(crate) totals: Value,
    pub(crate) quality: Value,
    pub(crate) passed: bool,
}
struct Checker<'a> {
    unit: &'a RequestUnit,
    checks: Vec<Check>,
}
impl Checker<'_> {
    fn add(&mut self, cat: &str, id: &str, ok: bool, code: &str, expected: Value, actual: Value) {
        self.checks.push(Check {
            category: cat.into(),
            object_id: id.into(),
            unit_id: self.unit.unit_id.clone(),
            start_byte: self.unit.start_byte,
            end_byte: self.unit.end_byte,
            status: if ok { "passed" } else { "failed" }.into(),
            code: if ok { None } else { Some(code.into()) },
            expected,
            actual,
        });
    }
    fn result(&self, id: &str, status: Option<&str>) -> ObjectResult {
        self.result_from(id, status, &self.checks)
    }
    fn result_from(&self, id: &str, status: Option<&str>, checks: &[Check]) -> ObjectResult {
        let issues = checks
            .iter()
            .filter(|c| c.object_id == id && c.status == "failed")
            .filter_map(|c| c.code.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        ObjectResult {
            object_id: id.into(),
            unit_id: self.unit.unit_id.clone(),
            start_byte: self.unit.start_byte,
            end_byte: self.unit.end_byte,
            status: status
                .unwrap_or(if !issues.is_empty() {
                    "failed"
                } else if checks
                    .iter()
                    .any(|check| check.object_id == id && check.status == "needs_review")
                {
                    "needs_review"
                } else {
                    "passed"
                })
                .into(),
            issue_codes: issues,
        }
    }
}
pub(crate) fn assess(
    r: &Review,
    g: &Gold,
    d: &Decision,
    c: Option<&SavedCandidate>,
    u: &RequestUnit,
    actual_binding: &crate::extraction_model::SourceBinding,
    schema_error: Option<&str>,
    strict_error: Option<&str>,
) -> Assessment {
    let mut a = Checker {
        unit: u,
        checks: vec![],
    };
    let expected_id = "expected-0";
    a.add(
        "schema",
        expected_id,
        c.is_some(),
        "VERIFY_SCHEMA",
        json!("strict_schema_1"),
        json!(schema_error),
    );
    let atoms = c.map(|c| c.payload.atoms.as_slice()).unwrap_or(&[]);
    let count = atoms.len();
    a.add(
        "source",
        expected_id,
        u.source_status == "parsed",
        "VERIFY_SOURCE_REVIEW",
        json!("parsed"),
        json!(u.source_status),
    );
    if u.source_status == "needs_review" {
        a.checks.last_mut().unwrap().status = "needs_review".into();
    }
    a.add(
        "exclusion",
        expected_id,
        d.disposition == Disposition::Included,
        "VERIFY_EXCLUSION",
        json!("included"),
        json!(d.disposition),
    );
    match d.disposition {
        Disposition::Pending => a.checks.last_mut().unwrap().status = "unprocessed".into(),
        Disposition::NeedsReview => a.checks.last_mut().unwrap().status = "needs_review".into(),
        _ => {}
    }
    let ids = atoms
        .iter()
        .map(|c| c.atom.atom_id.clone())
        .collect::<Vec<_>>();
    a.add(
        "coverage",
        expected_id,
        count == 1
            && d.unit_id == u.unit_id
            && d.atom_ids.len() == 1
            && ids.contains(&d.atom_ids[0]),
        if count == 0 {
            "VERIFY_MISSING"
        } else {
            "VERIFY_COVERAGE"
        },
        json!({"unit_id":u.unit_id,"atom_count":1}),
        json!({"decision":d,"candidate_ids":ids}),
    );
    if let Some(err) = strict_error {
        a.add(
            "strict_candidate",
            expected_id,
            false,
            err,
            json!("strict_candidates"),
            json!(err),
        );
    }
    let mut candidate_results = vec![];
    let mut seen = BTreeSet::new();
    let mut duplicates = 0;
    let mut additional_concepts = 0;
    let mut primary_end = None;
    if let Some(c) = c {
        a.add(
            "binding",
            expected_id,
            c.request_id == c.payload.request_id
                && c.binding == c.payload.binding
                && c.binding == *actual_binding,
            "VERIFY_BINDING",
            json!({"request_id":c.request_id,"binding":actual_binding}),
            json!({"request_id":c.payload.request_id,"binding":c.payload.binding}),
        );
        for (index, ca) in c.payload.atoms.iter().enumerate() {
            let candidate_start = a.checks.len();
            let id = &ca.atom.atom_id;
            if let Some(err) = strict_error {
                a.add(
                    "strict_candidate",
                    id,
                    false,
                    err,
                    json!("strict_candidates"),
                    json!(err),
                );
            }
            let parsed = body(&ca.atom.content, r, g);
            let key = if parsed.as_ref().is_some_and(|b| {
                same_set(&b.conditions, &g.conditions) && same_set(&b.exceptions, &g.exceptions)
            }) {
                "expected_fact".into()
            } else {
                normalized(&ca.atom.content)
            };
            let unique = seen.insert(key);
            if !unique {
                duplicates += 1;
            }
            a.add(
                "duplicate",
                id,
                unique,
                "VERIFY_DUPLICATE",
                json!("unique_semantic_atom"),
                json!(ca.atom.content),
            );
            let support = c
                .payload
                .evidence
                .iter()
                .filter(|e| {
                    ca.atom.evidence_links.iter().any(|l| {
                        l.relation == EvidenceRelation::Supports && l.evidence_id == e.evidence_id
                    })
                })
                .collect::<Vec<_>>();
            let quote_start = u
                .text
                .find(&g.quote)
                .map(|n| n + u.start_byte)
                .unwrap_or(u.end_byte);
            let quote_end = quote_start + g.quote.len();
            let evidence_valid = |e: &crate::extraction_model::CandidateEvidence| {
                e.unit_id == u.unit_id
                    && e.excerpt_start_byte < e.excerpt_end_byte
                    && e.excerpt_end_byte <= u.end_byte
                    && e.excerpt_start_byte
                        .checked_sub(u.start_byte)
                        .zip(e.excerpt_end_byte.checked_sub(u.start_byte))
                        .and_then(|(start, end)| u.text.get(start..end))
                        .is_some_and(|slice| {
                            slice == e.quote
                                && crate::text::sha256(slice.as_bytes()) == e.excerpt_sha256
                        })
            };
            for evidence in &support {
                a.add(
                    "evidence_identity",
                    &evidence.evidence_id,
                    evidence_valid(evidence),
                    "VERIFY_EVIDENCE",
                    json!({"unit_id":u.unit_id,"start_byte":u.start_byte,"end_byte":u.end_byte}),
                    json!(evidence),
                );
                let check = a.checks.last_mut().unwrap();
                check.start_byte = evidence.excerpt_start_byte;
                check.end_byte = evidence.excerpt_end_byte;
            }
            let supported = support.iter().all(|e| evidence_valid(e))
                && support.iter().any(|e| {
                    e.unit_id == u.unit_id
                        && e.excerpt_start_byte <= quote_start
                        && e.excerpt_end_byte >= quote_end
                        && e.quote.contains(&g.quote)
                });
            a.add(
                "evidence",
                id,
                supported,
                "VERIFY_EVIDENCE",
                json!({"quote":g.quote,"start_byte":quote_start,"end_byte":quote_end}),
                json!(support),
            );
            let linked = c
                .payload
                .concepts
                .iter()
                .filter(|co| ca.atom.concept_ids.contains(&co.concept_id))
                .collect::<Vec<_>>();
            let names = linked
                .iter()
                .flat_map(|co| std::iter::once(&co.canonical_name).chain(co.aliases.iter()))
                .map(|s| normalized(s))
                .collect::<BTreeSet<_>>();
            let compound = normalized(&g.concept_name);
            let group = names.contains(&compound)
                || (names.contains(&normalized(&r.subject))
                    && names.contains(&normalized(&r.property)));
            let text = support
                .iter()
                .map(|e| normalized(&e.quote))
                .collect::<String>();
            let extras_ok = c.payload.concepts.iter().all(|co| {
                std::iter::once(&co.canonical_name)
                    .chain(co.aliases.iter())
                    .all(|name| normalized(name) == compound || text.contains(&normalized(name)))
            });
            additional_concepts = c
                .payload
                .concepts
                .iter()
                .filter(|co| {
                    let n = normalized(&co.canonical_name);
                    n != compound && n != normalized(&r.subject) && n != normalized(&r.property)
                })
                .count();
            a.add(
                "concept",
                id,
                group && extras_ok,
                "VERIFY_CONCEPT",
                json!({"compound":g.concept_name,"components":[r.subject,r.property]}),
                json!({"linked":linked,"additional_source_concepts":additional_concepts}),
            );
            a.add(
                "content",
                id,
                parsed.as_ref().is_some_and(|b| {
                    same_set(&b.exceptions, &g.exceptions)
                        && b.conditions.iter().all(|item| {
                            g.conditions
                                .iter()
                                .chain(r.allowed_contexts.iter())
                                .any(|required| normalized(required) == normalized(item))
                        })
                        && g.conditions.iter().all(|item| {
                            b.conditions
                                .iter()
                                .any(|actual| normalized(actual) == normalized(item))
                        })
                        && set_ok(&b.conditions)
                }),
                "VERIFY_CONTENT",
                json!(g.content),
                json!(ca.atom.content),
            );
            let m = ca.measurement.as_ref();
            let fact_matches = m.is_some_and(|m| {
                m.value == g.value
                    && m.value_status
                        == if g.unknown {
                            ValueStatus::Unknown
                        } else {
                            ValueStatus::Known
                        }
            }) && parsed.is_some();
            a.add(
                "fact",
                id,
                fact_matches && !g.unknown,
                "VERIFY_FACT",
                json!({"value":g.value,"value_status":if g.unknown{"unknown"}else{"known"}}),
                json!(m),
            );
            if g.unknown && fact_matches {
                let check = a.checks.last_mut().unwrap();
                check.status = "needs_review".into();
                check.code = Some("VERIFY_UNKNOWN".into());
            }
            let qualifiers = &ca.atom.qualifiers;
            let unit_ok = m.is_some_and(|m| m.unit == g.unit)
                && g.unit.as_ref().is_none_or(|unit| {
                    qualifiers
                        .iter()
                        .filter(|q| normalized(q) == normalized(unit))
                        .count()
                        == 1
                });
            a.add(
                "unit",
                id,
                unit_ok,
                "VERIFY_UNIT",
                json!(g.unit),
                json!({"measurement":m.map(|m|&m.unit),"qualifiers":qualifiers}),
            );
            let conditions = m.map(|m| m.conditions.clone()).unwrap_or_default();
            let allowed = |s: &String| {
                g.conditions
                    .iter()
                    .chain(r.allowed_contexts.iter())
                    .any(|x| normalized(x) == normalized(s))
            };
            for condition in &g.conditions {
                let ok = conditions
                    .iter()
                    .filter(|x| normalized(x) == normalized(condition))
                    .count()
                    == 1
                    && qualifiers
                        .iter()
                        .filter(|x| normalized(x) == normalized(condition))
                        .count()
                        == 1
                    && parsed.as_ref().is_some_and(|b| {
                        b.conditions
                            .iter()
                            .filter(|x| normalized(x) == normalized(condition))
                            .count()
                            == 1
                    });
                a.add("condition",id,ok,"VERIFY_CONDITION",json!(condition),json!({"measurement":conditions,"qualifiers":qualifiers,"body":parsed.as_ref().map(|b|&b.conditions)}));
            }
            let condition_set = conditions.iter().all(allowed)
                && set_ok(&conditions)
                && parsed
                    .as_ref()
                    .is_some_and(|b| b.conditions.iter().all(allowed) && set_ok(&b.conditions));
            a.add(
                "condition_set",
                id,
                condition_set,
                "VERIFY_CONDITION",
                json!({"required":g.conditions,"allowed_contexts":r.allowed_contexts}),
                json!({"measurement":conditions,"body":parsed.as_ref().map(|b|&b.conditions)}),
            );
            for exception in &g.exceptions {
                let ok = qualifiers
                    .iter()
                    .filter(|x| normalized(x) == normalized(exception))
                    .count()
                    == 1
                    && parsed.as_ref().is_some_and(|b| {
                        b.exceptions
                            .iter()
                            .filter(|x| normalized(x) == normalized(exception))
                            .count()
                            == 1
                    });
                a.add(
                    "exception",
                    id,
                    ok,
                    "VERIFY_EXCEPTION",
                    json!(exception),
                    json!({"qualifiers":qualifiers,"body":parsed.as_ref().map(|b|&b.exceptions)}),
                );
            }
            a.add(
                "exception_set",
                id,
                parsed
                    .as_ref()
                    .is_some_and(|b| same_set(&b.exceptions, &g.exceptions)),
                "VERIFY_EXCEPTION",
                json!(g.exceptions),
                json!(parsed.as_ref().map(|b| &b.exceptions)),
            );
            let qvalid = qualifiers.iter().all(|s| {
                g.conditions
                    .iter()
                    .chain(g.exceptions.iter())
                    .chain(g.unit.iter())
                    .chain(r.allowed_contexts.iter())
                    .any(|x| normalized(x) == normalized(s))
            }) && qualifiers
                .iter()
                .map(|s| normalized(s))
                .collect::<BTreeSet<_>>()
                .len()
                == qualifiers.len();
            a.add("qualifiers",id,qvalid,"VERIFY_CONDITION",json!({"unit":g.unit,"conditions":g.conditions,"exceptions":g.exceptions,"contexts":r.allowed_contexts}),json!(qualifiers));
            let candidate_range = candidate_start..a.checks.len();
            candidate_results.push(a.result_from(id, None, &a.checks[candidate_range.clone()]));
            if index == 0 {
                primary_end = Some(candidate_range.end);
            }
        }
    }
    let mut status = if a.checks.iter().all(|c| c.status == "passed") {
        "passed"
    } else {
        "failed"
    };
    if c.is_none() {
        status = "failed";
    } else if count == 0 || d.disposition == Disposition::Pending {
        status = "unprocessed";
    } else if g.unknown || u.source_status != "parsed" || d.disposition == Disposition::NeedsReview
    {
        status = "needs_review";
    }
    if status == "needs_review" || status == "unprocessed" {
        for result in &mut candidate_results {
            if result.status == "passed" {
                result.status = status.into();
            }
        }
    }
    let mut result = a.result(expected_id, Some(status));
    result.issue_codes = a
        .checks
        .iter()
        .filter_map(|c| c.code.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let totals = json!({"expected_atoms":1,"passed":usize::from(status=="passed"),"failed":usize::from(status=="failed"),"needs_review":usize::from(status=="needs_review"),"unprocessed":usize::from(status=="unprocessed"),"candidate_atoms":count,"unexpected_atoms":count.saturating_sub(1),"duplicate_atoms":duplicates,"required_concept_groups":1,"additional_source_concepts":additional_concepts});
    let mut quality = BTreeMap::new();
    for (category, n) in [
        ("source", 1),
        ("schema", 1),
        ("binding", 1),
        ("concept", 1),
        ("fact", 1),
        ("content", 1),
        ("evidence", 1),
        ("condition", g.conditions.len()),
        ("unit", usize::from(g.unit.is_some())),
        ("exception", g.exceptions.len()),
        ("coverage", 1),
        ("exclusion", 1),
    ] {
        let matching = a.checks[..primary_end.unwrap_or(a.checks.len())]
            .iter()
            .filter(|check| check.category == category)
            .collect::<Vec<_>>();
        let count_status = |state: &str| {
            matching
                .iter()
                .take(n)
                .filter(|check| check.status == state)
                .count()
        };
        let passed = count_status("passed");
        let failed = count_status("failed");
        let needs_review = count_status("needs_review");
        let unprocessed = n - passed - failed - needs_review;
        quality.insert(category,json!({"expected":n,"passed":passed,"failed":failed,"needs_review":needs_review,"unprocessed":unprocessed,"status":if n==0{"not_applicable"}else if failed>0{"failed"}else if needs_review>0{"needs_review"}else if unprocessed>0{"unprocessed"}else{"passed"}}));
    }
    Assessment {
        checks: a.checks,
        expected: vec![result],
        candidates: candidate_results,
        totals,
        quality: json!(quality),
        passed: status == "passed",
    }
}
