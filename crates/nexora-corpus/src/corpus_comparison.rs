use crate::{CorpusError, CorpusFact, CorpusMismatch, CorpusScores};
use std::collections::BTreeMap;
pub struct CorpusComparison {
    pub scores: CorpusScores,
    pub mismatches: Vec<CorpusMismatch>,
}
impl CorpusComparison {
    pub fn compare(expected: &[CorpusFact], observed: &[CorpusFact]) -> Result<Self, CorpusError> {
        let mut counts: BTreeMap<String, (usize, usize)> = BTreeMap::new();
        for f in expected {
            counts.entry(serde_json::to_string(f)?).or_default().0 += 1;
        }
        for f in observed {
            counts.entry(serde_json::to_string(f)?).or_default().1 += 1;
        }
        let (mut tp, mut fp, mut missing) = (0, 0, 0);
        let mut mismatches = Vec::new();
        for (fact, (e, o)) in counts {
            tp += e.min(o);
            fp += o.saturating_sub(e);
            missing += e.saturating_sub(o);
            if e != o && mismatches.len() < 100 {
                mismatches.push(CorpusMismatch {
                    category: "fact".into(),
                    message: format!(
                        "expected {e}, observed {o}: {}",
                        fact.chars().take(256).collect::<String>()
                    ),
                });
            }
        }
        Ok(Self {
            scores: CorpusScores::new(tp, fp, missing),
            mismatches,
        })
    }
}
