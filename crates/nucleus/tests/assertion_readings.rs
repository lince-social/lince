use nucleus::{expr::{Expr,HAS_ASSERTION,MapResolver},karma::Condition};

#[test]
fn assertion_readings_bind_records_and_predicates_separately_and_reach_zero() {
    let source="has_assertion(@response, #answer, @answer) * 5";
    let condition=Condition::parse(source).unwrap();
    assert_eq!(condition.reads().len(),1);
    assert_eq!(condition.reads()[0].func,HAS_ASSERTION);
    assert_eq!(condition.reads()[0].slug,"response|answer|answer");
    let bound=condition.map_references(|reading,_|Ok(match reading {
        "has_assertion.subject"=>"r_subject",
        "has_assertion.predicate"=>"c_answer",
        "has_assertion.object"=>"r_answer",
        _=>panic!("Unexpected reading {reading}"),
    }.into())).unwrap();
    assert_eq!(bound.reads()[0].slug,"r_subject|c_answer|r_answer");
    let expression=Expr::parse(source).unwrap();
    for (present,points) in [(1.0,5.0),(0.0,0.0)] {
        let mut resolver=MapResolver::default();resolver.set(HAS_ASSERTION,"response|answer|answer",None,present);
        assert_eq!(expression.eval(&mut resolver).unwrap(),points);
    }
    assert!(Condition::parse("has_assertion(@response, #answer)").is_ok());
    for invalid in ["has_assertion(@response)","has_assertion(@response, 3)","has_assertion(@r, #a, @o, @extra)"] {
        assert!(Condition::parse(invalid).is_err());
    }
}
