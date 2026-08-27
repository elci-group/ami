//! Ami's PR decision gates (directive section 11). Each gate is evaluated
//! from real inputs — the candidate's declared status/evidence and a
//! measured transformation distance — never inferred silently. Gates 5
//! (market demand) and 8 (validation) are always NotEvaluated here: Ami has
//! no external demand-ingestion or validation-execution source wired up
//! yet, and a fabricated pass would be exactly the "ontological
//! hallucination" the directive warns against (section 17).

use crate::types::{
    CapabilityStatus, EvidenceConfidence, GateResult, TransformationCandidate,
    TransformationDistance,
};

/// Outcome of an actual validation run, when one was requested. `NotRun`
/// keeps gate 8 honest by default — Ami never assumes a candidate is
/// validated just because it was proposed.
#[derive(Debug, Clone)]
pub enum ValidationOutcome {
    NotRun,
    Passed,
    Failed(String),
}

pub fn evaluate(
    candidate: &TransformationCandidate,
    distance: &TransformationDistance,
    validation: &ValidationOutcome,
) -> Vec<GateResult> {
    vec![
        gate_1_capability_exists(candidate),
        gate_2_outcome_understood(candidate),
        gate_3_ontologically_valid(candidate),
        gate_4_reachable(candidate, distance),
        gate_5_demand_evidence(candidate),
        gate_6_identity_preserved(distance),
        gate_7_appropriately_scoped(distance),
        gate_8_validatable(validation),
    ]
}

fn gate_1_capability_exists(candidate: &TransformationCandidate) -> GateResult {
    match candidate.capability_status {
        CapabilityStatus::Existing => GateResult::pass(
            "1: capability exists",
            "capability is already present; prefer exposing, improving, or fixing it over rebuilding",
        ),
        other => GateResult::not_evaluated(
            "1: capability exists",
            format!("capability status is {other:?}, not Existing"),
        ),
    }
}

fn gate_2_outcome_understood(candidate: &TransformationCandidate) -> GateResult {
    if candidate.desired_outcome.trim().is_empty() {
        GateResult::fail(
            "2: outcome understood",
            "no desired outcome recorded; request should be classified as ambiguous",
        )
    } else {
        GateResult::pass("2: outcome understood", "desired outcome is stated")
    }
}

fn gate_3_ontologically_valid(candidate: &TransformationCandidate) -> GateResult {
    match candidate.capability_status {
        CapabilityStatus::Speculative => GateResult::fail(
            "3: ontologically valid",
            "capability status is Speculative — no project evidence supports it; reject or escalate",
        ),
        _ => GateResult::pass(
            "3: ontologically valid",
            "capability status is grounded in project evidence (existing/latent/reachable)",
        ),
    }
}

fn gate_4_reachable(
    candidate: &TransformationCandidate,
    distance: &TransformationDistance,
) -> GateResult {
    let high_identity_distance = distance.product_identity_distance >= 0.5;
    let established_path = matches!(
        candidate.capability_status,
        CapabilityStatus::Existing | CapabilityStatus::Reachable
    );
    if high_identity_distance && !established_path {
        GateResult::fail(
            "4: reachable",
            format!(
                "product identity distance is {:.2} with no established reachable path; do not disguise as a normal feature PR",
                distance.product_identity_distance
            ),
        )
    } else {
        GateResult::pass(
            "4: reachable",
            "transformation distance is within a bounded, reachable path",
        )
    }
}

fn gate_5_demand_evidence(candidate: &TransformationCandidate) -> GateResult {
    match candidate.evidence_confidence {
        EvidenceConfidence::Observed | EvidenceConfidence::Stated => GateResult::pass(
            "5: demand evidence",
            "evidence is observed or explicitly stated, not merely inferred",
        ),
        EvidenceConfidence::Inferred | EvidenceConfidence::Predicted => GateResult::not_evaluated(
            "5: demand evidence",
            "evidence is inferred/predicted only; classify this work as speculative",
        ),
        EvidenceConfidence::None => GateResult::not_evaluated(
            "5: demand evidence",
            "no market evidence recorded for this candidate",
        ),
    }
}

fn gate_6_identity_preserved(distance: &TransformationDistance) -> GateResult {
    if distance.product_identity_distance >= 0.75 {
        GateResult::fail(
            "6: identity preserved",
            format!(
                "product identity distance is {:.2}; this must be explicitly identified as an identity transformation, not routed as a normal PR",
                distance.product_identity_distance
            ),
        )
    } else {
        GateResult::pass(
            "6: identity preserved",
            "product identity distance is within normal evolutionary bounds",
        )
    }
}

fn gate_7_appropriately_scoped(distance: &TransformationDistance) -> GateResult {
    if distance.overall() > 0.6 {
        GateResult::fail(
            "7: appropriately scoped",
            format!(
                "overall transformation distance is {:.2}; decompose before proceeding",
                distance.overall()
            ),
        )
    } else {
        GateResult::pass(
            "7: appropriately scoped",
            "overall transformation distance fits a single-PR scope",
        )
    }
}

fn gate_8_validatable(validation: &ValidationOutcome) -> GateResult {
    match validation {
        ValidationOutcome::NotRun => GateResult::not_evaluated(
            "8: validatable",
            "no validation was run; supply one manually before implementing",
        ),
        ValidationOutcome::Passed => {
            GateResult::pass("8: validatable", "the project's test suite passed")
        }
        ValidationOutcome::Failed(detail) => GateResult::fail(
            "8: validatable",
            format!("the project's test suite failed: {detail}"),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::GateStatus;

    fn candidate(
        status: CapabilityStatus,
        evidence: EvidenceConfidence,
    ) -> TransformationCandidate {
        TransformationCandidate {
            id: "test".to_string(),
            description: "desc".to_string(),
            desired_outcome: "outcome".to_string(),
            required_capability: "cap".to_string(),
            capability_status: status,
            market_evidence: Vec::new(),
            evidence_confidence: evidence,
        }
    }

    fn zero_distance() -> TransformationDistance {
        TransformationDistance {
            component_distance: 0.0,
            architecture_distance: 0.0,
            capability_distance: 0.0,
            product_identity_distance: 0.0,
        }
    }

    #[test]
    fn speculative_capability_fails_ontology_gate() {
        let candidate = candidate(CapabilityStatus::Speculative, EvidenceConfidence::None);
        let results = evaluate(&candidate, &zero_distance(), &ValidationOutcome::NotRun);
        let gate3 = results.iter().find(|g| g.gate.starts_with("3")).unwrap();
        assert_eq!(gate3.status, GateStatus::Fail);
    }

    #[test]
    fn high_identity_distance_without_established_path_fails_reachability() {
        let candidate = candidate(CapabilityStatus::Latent, EvidenceConfidence::Observed);
        let mut distance = zero_distance();
        distance.product_identity_distance = 0.8;
        let results = evaluate(&candidate, &distance, &ValidationOutcome::NotRun);
        let gate4 = results.iter().find(|g| g.gate.starts_with("4")).unwrap();
        assert_eq!(gate4.status, GateStatus::Fail);
        let gate6 = results.iter().find(|g| g.gate.starts_with("6")).unwrap();
        assert_eq!(gate6.status, GateStatus::Fail);
    }

    #[test]
    fn gates_five_and_eight_are_never_a_fabricated_pass() {
        let candidate = candidate(CapabilityStatus::Existing, EvidenceConfidence::Observed);
        let results = evaluate(&candidate, &zero_distance(), &ValidationOutcome::NotRun);
        let gate5 = results.iter().find(|g| g.gate.starts_with("5")).unwrap();
        assert_eq!(gate5.status, GateStatus::Pass); // observed evidence *was* supplied here
        let gate8 = results.iter().find(|g| g.gate.starts_with("8")).unwrap();
        assert_eq!(gate8.status, GateStatus::NotEvaluated);
    }

    #[test]
    fn validation_outcome_drives_gate_eight_directly() {
        let candidate = candidate(CapabilityStatus::Existing, EvidenceConfidence::Observed);
        let results = evaluate(&candidate, &zero_distance(), &ValidationOutcome::Passed);
        let gate8 = results.iter().find(|g| g.gate.starts_with("8")).unwrap();
        assert_eq!(gate8.status, GateStatus::Pass);

        let results = evaluate(
            &candidate,
            &zero_distance(),
            &ValidationOutcome::Failed("2 tests failed".to_string()),
        );
        let gate8 = results.iter().find(|g| g.gate.starts_with("8")).unwrap();
        assert_eq!(gate8.status, GateStatus::Fail);
    }
}
