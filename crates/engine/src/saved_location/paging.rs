use super::*;
use serde_json::Value;

impl Engine {
    pub(super) async fn saved_location_page(
        &self,
        query: Option<Value>,
        mut records: Vec<String>,
        after: Option<String>,
        generation: String,
        bounds: [f64; 4],
        actor: Option<&str>,
    ) -> Result<ActionOutcome, EngineError> {
        let started = std::time::Instant::now();
        let invalid = |message: &str| EngineError::Consequence(message.into());
        if generation.is_empty()
            || generation.len() > 128
            || records.len() > 10_000
            || records.iter().any(|uid| !nucleus::valid_uid(uid, "r"))
            || bounds.iter().any(|number| !number.is_finite())
            || bounds[0].abs() > 180.0
            || bounds[2].abs() > 180.0
            || bounds[1].abs() > 90.0
            || bounds[3].abs() > 90.0
            || bounds[1] > bounds[3]
        {
            return Err(invalid(
                "Choose a valid map source, viewport and generation",
            ));
        }
        if query.as_ref().is_some_and(|query| {
            serde_json::to_vec(query).map_or(true, |bytes| bytes.len() > 64 * 1024)
        }) {
            return Err(invalid("The map filter exceeds its size limit"));
        }
        records.sort();
        records.dedup();
        let scope = nucleus::fact::sha256_hex(&serde_json::to_vec(&json!([
            actor, query, records, generation, bounds
        ]))?);
        let after = after
            .map(|cursor| {
                let (binding, uid) = cursor
                    .split_once(':')
                    .ok_or_else(|| invalid("Invalid map continuation"))?;
                if binding != scope || !nucleus::valid_uid(uid, "r") {
                    return Err(invalid(
                        "Map continuation belongs to another viewer, filter or viewport",
                    ));
                }
                Ok::<_, EngineError>(uid.to_owned())
            })
            .transpose()?;
        let explicit = query.is_none();
        let mut protein: protein::Protein =
            serde_json::from_value(query.unwrap_or_else(|| json!({"source":"record"})))?;
        if protein.source != protein::Source::Record || protein.aggregate.is_some() {
            return Err(invalid("Choose a Record Protein for map locations"));
        }
        let mut candidate_last = None;
        if explicit {
            let page: Vec<_> = records
                .into_iter()
                .filter(|uid| after.as_ref().is_none_or(|after| uid > after))
                .take(128)
                .map(protein::Predicate::UidEq)
                .collect();
            if page.is_empty() {
                return Ok(ActionOutcome {
                    data: Some(
                        json!({"features":[],"next_after":null,"generation":generation,"scope":scope,"query_micros":started.elapsed().as_micros().min(u128::from(u64::MAX)) as u64}),
                    ),
                    ..Default::default()
                });
            }
            candidate_last = page.last().and_then(|predicate| {
                if let protein::Predicate::UidEq(uid) = predicate {
                    Some(uid.clone())
                } else {
                    None
                }
            });
            protein.filter.push(protein::Predicate::Any(page));
        }
        protein.fields = Some(vec!["uid".into(), "head".into()]);
        protein.include = Default::default();
        protein.include.record_after = (!explicit).then_some(after.clone()).flatten();
        protein.order = vec![protein::Order::Asc("head".into())];
        protein.limit = Some(128);
        protein::validate(&protein)?;
        let rows = protein::execute_for(&self.store, &protein, actor).await?;
        let last = candidate_last.or_else(|| {
            rows.last()
                .and_then(|row| row["uid"].as_str())
                .map(str::to_owned)
        });
        let visible = self.saved_location_readable(actor).await?;
        let selected = rows
            .iter()
            .filter_map(|row| row["uid"].as_str().map(str::to_owned))
            .collect::<Vec<_>>();
        let mut locations = store::places::saved_locations_for_person_among(
            &self.store.pool,
            visible.as_ref(),
            actor,
            &selected,
        )
        .await?;
        let mut features = Vec::new();
        for row in &rows {
            let Some(uid) = row["uid"].as_str() else {
                continue;
            };
            let selected = locations.remove(uid).and_then(|location| location.selected);
            let selected = selected.filter(|location| {
                let padding = f64::from(location.radius_metres.unwrap_or_default()) / 111_195.0;
                let longitude_padding =
                    padding / location.latitude.to_radians().cos().abs().max(0.001);
                let latitude = location.latitude + padding >= bounds[1]
                    && location.latitude - padding <= bounds[3];
                let longitude = bounds[0] <= bounds[2]
                    && location.longitude + longitude_padding >= bounds[0]
                    && location.longitude - longitude_padding <= bounds[2]
                    || bounds[0] > bounds[2]
                        && (location.longitude + longitude_padding >= bounds[0]
                            || location.longitude - longitude_padding <= bounds[2])
                    || location.longitude - longitude_padding < -180.0
                        && location.longitude - longitude_padding + 360.0 <= bounds[2]
                    || location.longitude + longitude_padding > 180.0
                        && location.longitude + longitude_padding - 360.0 >= bounds[0];
                latitude && longitude
            });
            features.push(json!({"record":uid,"head":row["head"],"location":selected}));
        }
        let next = last.map(|uid| format!("{scope}:{uid}"));
        Ok(ActionOutcome {
            data: Some(
                json!({"features":features,"next_after":next,"generation":generation,"scope":scope,"page_size":128,"query_micros":started.elapsed().as_micros().min(u128::from(u64::MAX)) as u64}),
            ),
            ..Default::default()
        })
    }
}
