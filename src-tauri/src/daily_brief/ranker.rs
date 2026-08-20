use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};

use super::models::{BriefItemKind, BriefSectionCounts, DailyBriefItem, DAILY_BRIEF_DETAIL_LIMIT};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct RankedBrief {
    pub focus_item: Option<DailyBriefItem>,
    pub attention_items: Vec<DailyBriefItem>,
    pub detail_items: Vec<DailyBriefItem>,
    pub section_counts: BriefSectionCounts,
    pub actionable_count: usize,
}

fn compare_items(left: &DailyBriefItem, right: &DailyBriefItem) -> Ordering {
    right
        .priority
        .cmp(&left.priority)
        .then_with(|| match (left.due_at, right.due_at) {
            (Some(a), Some(b)) => a.cmp(&b),
            (Some(_), None) => Ordering::Less,
            (None, Some(_)) => Ordering::Greater,
            (None, None) => Ordering::Equal,
        })
        .then_with(|| right.occurred_at.cmp(&left.occurred_at))
        .then_with(|| left.item_id.cmp(&right.item_id))
}

fn should_replace(current: &DailyBriefItem, candidate: &DailyBriefItem) -> bool {
    compare_items(candidate, current) == Ordering::Less
}

fn counts_for(items: &[DailyBriefItem]) -> BriefSectionCounts {
    let mut counts = BriefSectionCounts::default();
    for item in items {
        if item.requires_attention {
            counts.attention += 1;
        }
        match item.kind {
            BriefItemKind::Due | BriefItemKind::Schedule => counts.today += 1,
            BriefItemKind::Progress | BriefItemKind::ContinueConversation => {
                counts.in_progress += 1
            }
            BriefItemKind::Change => counts.changes += 1,
            BriefItemKind::Insight => counts.insights += 1,
            BriefItemKind::Approval | BriefItemKind::Risk => {}
        }
    }
    counts
}

pub fn rank(mut candidates: Vec<DailyBriefItem>) -> RankedBrief {
    let mut by_canonical: HashMap<String, DailyBriefItem> = HashMap::new();
    for item in candidates.drain(..) {
        let key = item.canonical_ref.clone();
        match by_canonical.get(&key) {
            Some(current) if !should_replace(current, &item) => {}
            _ => {
                by_canonical.insert(key, item);
            }
        }
    }

    let mut items: Vec<DailyBriefItem> = by_canonical.into_values().collect();
    items.sort_by(compare_items);
    let section_counts = counts_for(&items);
    let actionable_count = items.iter().filter(|item| item.requires_attention).count();

    let focus_item = items.first().cloned();
    let focus_id = focus_item.as_ref().map(|item| item.item_id.as_str());
    let attention_items: Vec<DailyBriefItem> = items
        .iter()
        .filter(|item| Some(item.item_id.as_str()) != focus_id && item.requires_attention)
        .take(2)
        .cloned()
        .collect();
    let selected_ids: HashSet<String> = focus_item
        .iter()
        .chain(attention_items.iter())
        .map(|item| item.item_id.clone())
        .collect();
    let detail_items = items
        .into_iter()
        .filter(|item| !selected_ids.contains(&item.item_id))
        .take(DAILY_BRIEF_DETAIL_LIMIT)
        .collect();

    RankedBrief {
        focus_item,
        attention_items,
        detail_items,
        section_counts,
        actionable_count,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::Value;

    use super::*;
    use crate::daily_brief::models::{
        BriefActionKind, BriefSource, DailyBriefAction, DailyBriefItem,
    };

    fn item(id: &str, canonical: &str, priority: i32, attention: bool) -> DailyBriefItem {
        DailyBriefItem {
            item_id: DailyBriefItem::stable_id(BriefSource::WorkCore, id),
            canonical_ref: canonical.into(),
            source: BriefSource::WorkCore,
            source_id: id.into(),
            source_revision: "1".into(),
            kind: if attention {
                BriefItemKind::Approval
            } else {
                BriefItemKind::Progress
            },
            title: Some(id.into()),
            title_key: None,
            summary: None,
            summary_key: None,
            message_args: Value::Null,
            priority,
            requires_attention: attention,
            occurred_at: Some(priority as i64),
            due_at: None,
            action: DailyBriefAction {
                kind: BriefActionKind::OpenWorkObject,
                target_type: Some("work_object".into()),
                target_id: Some(id.into()),
                payload: Value::Null,
            },
            reason_codes: vec![],
            evidence_refs: vec![],
        }
    }

    #[test]
    fn enforces_one_focus_and_two_attention_items() {
        let result = rank(vec![
            item("a", "a", 1000, true),
            item("b", "b", 900, true),
            item("c", "c", 800, true),
            item("d", "d", 700, true),
        ]);
        assert_eq!(result.focus_item.unwrap().source_id, "a");
        assert_eq!(result.attention_items.len(), 2);
        assert_eq!(result.attention_items[0].source_id, "b");
        assert_eq!(result.attention_items[1].source_id, "c");
        assert_eq!(result.detail_items.len(), 1);
        assert_eq!(result.actionable_count, 4);
    }

    #[test]
    fn deduplicates_by_canonical_reference_and_keeps_higher_priority() {
        let result = rank(vec![
            item("work_goal", "goal:g1", 500, false),
            item("runtime", "goal:g1", 950, true),
        ]);
        assert_eq!(result.focus_item.unwrap().source_id, "runtime");
        assert!(result.detail_items.is_empty());
    }

    #[test]
    fn ordering_is_stable_for_equal_candidates() {
        let first = rank(vec![item("b", "b", 500, false), item("a", "a", 500, false)]);
        let second = rank(vec![item("a", "a", 500, false), item("b", "b", 500, false)]);
        assert_eq!(
            first.focus_item.unwrap().item_id,
            second.focus_item.unwrap().item_id
        );
    }

    #[test]
    fn section_counts_cover_hidden_detail_items() {
        let mut due = item("due", "due", 700, true);
        due.kind = BriefItemKind::Due;
        let mut insight = item("insight", "insight", 100, false);
        insight.kind = BriefItemKind::Insight;
        let result = rank(vec![due, insight]);
        assert_eq!(result.section_counts.attention, 1);
        assert_eq!(result.section_counts.today, 1);
        assert_eq!(result.section_counts.insights, 1);
    }
}
