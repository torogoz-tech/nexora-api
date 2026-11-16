use crate::EvaluationStatus::{self, Fail, Pass, Unknown};
#[test]
fn three_valued_truth_tables_are_exhaustive() {
    let states = [Pass, Fail, Unknown];
    let all = [
        [Pass, Fail, Unknown],
        [Fail, Fail, Fail],
        [Unknown, Fail, Unknown],
    ];
    let any = [
        [Pass, Pass, Pass],
        [Pass, Fail, Unknown],
        [Pass, Unknown, Unknown],
    ];
    for (i, a) in states.iter().enumerate() {
        for (j, b) in states.iter().enumerate() {
            assert_eq!(EvaluationStatus::all(&[*a, *b]), all[i][j]);
            assert_eq!(EvaluationStatus::any(&[*a, *b]), any[i][j]);
            assert_eq!(EvaluationStatus::none(&[*a, *b]), any[i][j].negate());
        }
    }
}
