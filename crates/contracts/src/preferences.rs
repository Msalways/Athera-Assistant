use crate::adaptive::{
    AdaptiveRule, RuleProposal, RuleScope, RuleSource, ADAPTIVE_RULE_SCHEMA_V1,
    RULE_PROPOSAL_SCHEMA_V1,
};
use crate::personalization::{Observation, ObservationKind};
use crate::Id;

pub fn propose_from_observations(
    observations: &[Observation],
    min_repeats: usize,
    now_millis: u64,
) -> Vec<RuleProposal> {
    let mut groups: Vec<(String, Vec<&Observation>)> = Vec::new();
    for observation in observations.iter().filter(|observation| {
        matches!(
            observation.kind,
            ObservationKind::Correction | ObservationKind::Remember
        ) && !observation.text.trim().is_empty()
    }) {
        let key = observation.text.trim().to_lowercase();
        match groups.iter_mut().find(|(text, _)| *text == key) {
            Some((_, group)) => group.push(observation),
            None => groups.push((key, vec![observation])),
        }
    }
    groups
        .into_iter()
        .filter(|(_, group)| group.len() >= min_repeats.max(2))
        .map(|(_, group)| {
            let evidence_ids = group.iter().map(|item| item.id).collect::<Vec<_>>();
            RuleProposal {
                schema: RULE_PROPOSAL_SCHEMA_V1.into(),
                id: Id::new_v4(),
                rule: AdaptiveRule {
                    schema: ADAPTIVE_RULE_SCHEMA_V1.into(),
                    id: Id::new_v4(),
                    version: 1,
                    scope: RuleScope::Global,
                    status: crate::adaptive::AdaptiveRuleStatus::Proposed,
                    source: RuleSource::System,
                    priority: 50,
                    instruction: group[0].text.trim().into(),
                    preference_key: None,
                    evidence_ids,
                    created_at: now_millis,
                    updated_at: now_millis,
                    expires_at: None,
                    supersedes: None,
                },
                rationale: format!(
                    "Observed {} times across conversations; awaiting review.",
                    group.len()
                ),
                proposed_at: now_millis,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn observation(kind: ObservationKind, text: &str) -> Observation {
        Observation {
            id: Id::new_v4(),
            conversation_id: Id::new_v4(),
            source_id: Id::new_v4(),
            kind,
            text: text.into(),
            created_at: 1000,
        }
    }

    #[test]
    fn repeated_correction_proposes_rule() {
        let observations = vec![
            observation(ObservationKind::Correction, "Prefer concise answers"),
            observation(ObservationKind::Remember, "prefer CONCISE answers "),
        ];
        let proposals = propose_from_observations(&observations, 2, 2000);
        assert_eq!(proposals.len(), 1);
        let proposal = &proposals[0];
        proposal.validate().unwrap();
        assert_eq!(proposal.rule.instruction, "Prefer concise answers");
        assert_eq!(proposal.rule.evidence_ids.len(), 2);
        assert!(proposal.rationale.contains('2'));
    }

    #[test]
    fn single_observation_proposes_nothing() {
        let observations = vec![observation(ObservationKind::Remember, "Prefer concise")];
        assert!(propose_from_observations(&observations, 2, 2000).is_empty());
    }

    #[test]
    fn non_signal_kinds_ignored() {
        let observations = vec![
            observation(ObservationKind::Outcome, "Prefer concise"),
            observation(ObservationKind::Rejection, "Prefer concise"),
            observation(ObservationKind::PreferenceFollowed, "Prefer concise"),
        ];
        assert!(propose_from_observations(&observations, 2, 2000).is_empty());
    }

    #[test]
    fn distinct_texts_propose_separately() {
        let observations = vec![
            observation(ObservationKind::Remember, "Be concise"),
            observation(ObservationKind::Remember, "Be concise"),
            observation(ObservationKind::Remember, "Use metric units"),
            observation(ObservationKind::Remember, "Use metric units"),
        ];
        let proposals = propose_from_observations(&observations, 2, 2000);
        assert_eq!(proposals.len(), 2);
    }

    #[test]
    fn higher_threshold_needs_more_repeats() {
        let observations = vec![
            observation(ObservationKind::Remember, "Be concise"),
            observation(ObservationKind::Remember, "Be concise"),
        ];
        assert!(propose_from_observations(&observations, 3, 2000).is_empty());
    }

    #[test]
    fn min_repeats_floor_is_two() {
        let observations = vec![observation(ObservationKind::Remember, "Be concise")];
        assert!(propose_from_observations(&observations, 0, 2000).is_empty());
        assert!(propose_from_observations(&observations, 1, 2000).is_empty());
    }
}
