use std::collections::{BTreeMap, BTreeSet};

use nucleus::karma::{
    Carry, Condition, ConditionError, Consequence, DecimalValue, ExactResolver, Gate,
};

struct Stock(BTreeMap<String, DecimalValue>);

impl ExactResolver for Stock {
    fn lookup(
        &mut self,
        function: &str,
        slug: &str,
        window: Option<i64>,
    ) -> Result<DecimalValue, ConditionError> {
        assert_eq!(function, "quantity");
        assert_eq!(window, None);
        self.0
            .get(slug)
            .copied()
            .ok_or_else(|| ConditionError::UnknownReference(slug.into()))
    }
}

fn number(value: &str) -> DecimalValue {
    DecimalValue::parse_inferred(value).unwrap()
}

#[test]
fn drinking_water_purchase_need_tracks_shortfalls_and_clears_at_the_target() {
    let source = include_str!("../../../institute/trails/habits/simple/Supply.lingua");
    let (identified, _) = anicca::ensure_uids(source).unwrap();
    let project = anicca::project(&anicca::parse(&identified).unwrap()).unwrap();
    let mut stock = Stock(BTreeMap::new());
    for record in &project.records {
        if let Some((quantity, unit)) = &record.quantity
            && unit.as_deref() == Some("liters")
        {
            stock
                .0
                .insert(record.slug.clone().unwrap(), number(quantity));
        }
    }
    assert_eq!(stock.0.len(), 3);
    assert_eq!(stock.0["wanted-drinking-water-supply"], number("6"));
    assert_eq!(stock.0["drinking-water"], number("0"));
    assert_eq!(stock.0["buy-drinking-water"], number("-6"));
    assert_eq!(project.rules.len(), 2);
    let rules: Vec<_> = project
        .rules
        .iter()
        .map(|rule| {
            assert_eq!(rule.quantity, 1);
            assert_eq!(rule.record_slug, "buy-drinking-water");
            assert!(rule.frequency_slug.is_empty());
            let condition = Condition::parse(rule.condition.as_deref().unwrap()).unwrap();
            let reads: BTreeSet<_> = condition
                .reads()
                .into_iter()
                .map(|reading| (reading.func, reading.slug))
                .collect();
            assert_eq!(
                reads,
                BTreeSet::from([
                    ("quantity".into(), "drinking-water".into()),
                    ("quantity".into(), "wanted-drinking-water-supply".into()),
                ])
            );
            let consequences: Vec<Consequence> =
                serde_json::from_str(&rule.consequences_json).unwrap();
            assert_eq!(consequences, vec![Consequence::SetQuantity { value: None }]);
            (
                condition,
                Gate::parse(rule.gate.as_deref().unwrap()).unwrap(),
                Carry::parse(rule.carry.as_deref().unwrap()).unwrap(),
            )
        })
        .collect();
    for (wanted, available, expected) in [
        ("6", "0", "-6"),
        ("6", "4", "-2"),
        ("6", "4", "-2"),
        ("6", "5.5", "-0.5"),
        ("6", "6", "0"),
        ("6", "8", "0"),
        ("6", "1.25", "-4.75"),
        ("10", "1.25", "-8.75"),
        ("10", "10", "0"),
        ("6", "10", "0"),
    ] {
        stock
            .0
            .insert("wanted-drinking-water-supply".into(), number(wanted));
        stock.0.insert("drinking-water".into(), number(available));
        let mut applied = 0;
        for (condition, gate, carry) in &rules {
            if let Some(value) = nucleus::karma::decide(condition, gate, carry, &mut stock).unwrap()
            {
                stock.0.insert("buy-drinking-water".into(), value);
                applied += 1;
            }
        }
        assert_eq!(applied, 1);
        assert_eq!(stock.0["buy-drinking-water"], number(expected));
        assert_eq!(stock.0["drinking-water"], number(available));
    }
}
