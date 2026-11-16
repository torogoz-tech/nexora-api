use nexora_corpus::{CorpusComparison, CorpusFact};
use nexora_graph::NodeKind;
fn fact(name: &str) -> CorpusFact {
    CorpusFact::Declaration {
        file: "source.txt".into(),
        kind: NodeKind::Function,
        name: name.into(),
        owner: Vec::new(),
        source: format!("DEF {name}"),
    }
}
#[test]
fn false_positives_false_negatives_and_duplicates_affect_scores() {
    let result =
        CorpusComparison::compare(&[fact("a"), fact("b")], &[fact("a"), fact("a"), fact("c")])
            .unwrap();
    assert_eq!(result.scores.true_positives, 1);
    assert_eq!(result.scores.false_positives, 2);
    assert_eq!(result.scores.false_negatives, 1);
    assert_eq!(result.scores.precision, Some(1.0 / 3.0));
    assert_eq!(result.scores.recall, Some(0.5));
    assert!(!result.mismatches.is_empty());
}
#[test]
fn empty_denominators_are_undefined_not_perfect() {
    let result = CorpusComparison::compare(&[], &[]).unwrap();
    assert_eq!(result.scores.precision, None);
    assert_eq!(result.scores.recall, None);
    let result = CorpusComparison::compare(&[fact("a")], &[]).unwrap();
    assert_eq!(result.scores.precision, None);
    assert_eq!(result.scores.recall, Some(0.0));
}
#[test]
fn replicated_symbol_names_are_not_treated_as_file_paths_and_labels_affect_scoring() {
    let fact:CorpusFact=serde_json::from_value(serde_json::json!({"type":"relation","file":"source.js","kind":"EXPORTS","target":"Object.run","resolution":"Resolved","source":"PUBLIC Object.run;","label":"public"})).unwrap();
    let prefixed = fact.prefixed("replica-0000");
    let value = serde_json::to_value(&prefixed).unwrap();
    assert_eq!(value["file"], "replica-0000/source.js");
    assert_eq!(value["target"], "Object.run");
    assert_eq!(value["label"], "public");
    let mut wrong = serde_json::to_value(&fact).unwrap();
    wrong["label"] = serde_json::json!("different");
    let wrong: CorpusFact = serde_json::from_value(wrong).unwrap();
    let comparison = CorpusComparison::compare(&[fact], &[wrong]).unwrap();
    assert_eq!(comparison.scores.false_positives, 1);
    assert_eq!(comparison.scores.false_negatives, 1);
}
#[test]
fn hierarchical_owners_distinguish_same_name_bindings_and_survive_replication() {
    let expected:CorpusFact=serde_json::from_value(serde_json::json!({"type":"relation","file":"source.nxb","kind":"REFERENCES","target":"value","resolution":"Resolved","source":"READ value;","owner":["outer","BLOCK"],"target_owner":["outer","BLOCK"]})).unwrap();
    let mut wrong = serde_json::to_value(&expected).unwrap();
    wrong["target_owner"] = serde_json::json!(["outer"]);
    let wrong: CorpusFact = serde_json::from_value(wrong).unwrap();
    let comparison = CorpusComparison::compare(std::slice::from_ref(&expected), &[wrong]).unwrap();
    assert_eq!(comparison.scores.false_positives, 1);
    assert_eq!(comparison.scores.false_negatives, 1);
    let replicated = serde_json::to_value(expected.prefixed("replica-0000")).unwrap();
    assert_eq!(replicated["file"], "replica-0000/source.nxb");
    assert_eq!(replicated["owner"], serde_json::json!(["outer", "BLOCK"]));
    assert_eq!(
        replicated["target_owner"],
        serde_json::json!(["outer", "BLOCK"])
    );
    assert_eq!(replicated["target"], "value");
}
