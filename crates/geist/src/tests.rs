use super::*;

fn batch() -> Vec<Transition<2, 1>> {
    (-4..=4)
        .map(|i| {
            let x = i as f32 / 4.0;
            Transition {
                observation: [x, x * x],
                action: [0.25 * x],
                next_observation: [0.8 * x, 0.5 * x * x],
            }
        })
        .collect()
}

#[test]
fn gradients_match_finite_differences_with_detached_targets() {
    let model = WorldModel::<2, 3, 1>::new(
        Config {
            variance_floor: 0.9,
            ..Config::default()
        },
        42,
    )
    .unwrap();
    let batch = batch();
    let (_, gradients) = model.objective(&batch).unwrap();
    for (index, analytic) in gradients.values().enumerate() {
        let mut plus = model.clone();
        let mut minus = model.clone();
        *plus.online.values_mut().nth(index).unwrap() += 0.001;
        *minus.online.values_mut().nth(index).unwrap() -= 0.001;
        let loss = |model: &WorldModel<2, 3, 1>| {
            let metrics = model.evaluate(&batch).unwrap();
            metrics.prediction_loss + model.config.variance_weight * metrics.variance_loss
        };
        let numerical = (loss(&plus) - loss(&minus)) / 0.002;
        assert!(
            (analytic - numerical).abs() < 0.0003,
            "parameter {index}: analytic {analytic}, numerical {numerical}"
        );
    }
}

#[test]
fn teacher_tracks_updated_encoder_by_ema() {
    let mut model = WorldModel::<2, 3, 1>::new(
        Config {
            target_momentum: 0.75,
            ..Config::default()
        },
        7,
    )
    .unwrap();
    let old = model.clone();
    model.train_step(&batch()).unwrap();
    for ((target, previous), online) in model
        .target_encoder
        .iter()
        .flatten()
        .chain(model.target_bias.iter())
        .zip(
            old.target_encoder
                .iter()
                .flatten()
                .chain(old.target_bias.iter()),
        )
        .zip(
            model
                .online
                .encoder
                .iter()
                .flatten()
                .chain(model.online.encoder_bias.iter()),
        )
    {
        assert_eq!(*target, 0.75 * previous + 0.25 * online);
    }
}

#[test]
fn learns_against_frozen_teacher_without_constant_embeddings() {
    let mut model = WorldModel::<2, 3, 1>::new(
        Config {
            learning_rate: 0.1,
            target_momentum: 1.0,
            variance_weight: 0.0,
            ..Config::default()
        },
        42,
    )
    .unwrap();
    let batch = batch();
    let target = model.target(&[0.3, 0.7]).unwrap();
    let before = model.evaluate(&batch).unwrap().prediction_loss;
    for _ in 0..1000 {
        model.train_step(&batch).unwrap();
    }
    let after = model.evaluate(&batch).unwrap();
    assert!(
        after.prediction_loss < before * 0.1,
        "{before} -> {}",
        after.prediction_loss
    );
    assert!(after.latent_std > 0.05);
    assert_eq!(target, model.target(&[0.3, 0.7]).unwrap());
}

#[test]
fn invalid_batch_does_not_mutate_parameters_or_teacher() {
    let mut model = WorldModel::<2, 3, 1>::new(Config::default(), 42).unwrap();
    let before = model.clone();
    let mut invalid = batch();
    invalid[3].action[0] = f32::NAN;
    assert_eq!(model.train_step(&invalid), Err(Error::NonFinite));
    assert_eq!(
        model.online.values().collect::<Vec<_>>(),
        before.online.values().collect::<Vec<_>>()
    );
    assert_eq!(model.target_encoder, before.target_encoder);
    assert_eq!(model.target_bias, before.target_bias);
    assert_eq!(model.train_step(&[]), Err(Error::BatchTooSmall));
}

#[test]
fn rejects_invalid_dimensions_and_configuration() {
    assert!(matches!(
        WorldModel::<0, 3, 1>::new(Config::default(), 0),
        Err(Error::InvalidDimensions)
    ));
    assert!(matches!(
        WorldModel::<2, 0, 1>::new(Config::default(), 0),
        Err(Error::InvalidDimensions)
    ));
    for config in [
        Config {
            learning_rate: 0.0,
            ..Config::default()
        },
        Config {
            target_momentum: 1.1,
            ..Config::default()
        },
        Config {
            variance_weight: f32::NAN,
            ..Config::default()
        },
        Config {
            variance_floor: -0.1,
            ..Config::default()
        },
        Config {
            gradient_clip: f32::INFINITY,
            ..Config::default()
        },
    ] {
        assert!(matches!(
            WorldModel::<2, 3, 1>::new(config, 0),
            Err(Error::InvalidConfig)
        ));
    }
}

#[test]
fn seed_reproduces_initialization_and_actions_change_rollout() {
    let model = WorldModel::<2, 3, 1>::new(Config::default(), 0).unwrap();
    let duplicate = WorldModel::<2, 3, 1>::new(Config::default(), 0).unwrap();
    let left = model.rollout(&[0.2, 0.4], &[[-0.5]; 4]).unwrap();
    let right = model.rollout(&[0.2, 0.4], &[[0.5]; 4]).unwrap();
    assert_ne!(left, right);
    assert_eq!(right, duplicate.rollout(&[0.2, 0.4], &[[0.5]; 4]).unwrap());
    let mut latent = model.encode(&[0.2, 0.4]).unwrap();
    for expected in &right {
        latent = model.predict(&latent, &[0.5]).unwrap();
        assert_eq!(latent, *expected);
    }
    assert!(model.rollout(&[0.2, 0.4], &[]).unwrap().is_empty());
}

#[test]
fn supports_unconditioned_prediction() {
    let mut model = WorldModel::<2, 3, 0>::new(Config::default(), 5).unwrap();
    let batch = [
        Transition {
            observation: [0.0, 1.0],
            action: [],
            next_observation: [0.2, 0.8],
        },
        Transition {
            observation: [1.0, 0.0],
            action: [],
            next_observation: [0.8, 0.2],
        },
    ];
    assert!(
        model
            .train_step(&batch)
            .unwrap()
            .prediction_loss
            .is_finite()
    );
    assert_eq!(model.rollout(&[0.0, 1.0], &[[]; 3]).unwrap().len(), 3);
}

#[test]
fn rejects_overflow_before_activation_can_hide_it() {
    let mut model = WorldModel::<2, 3, 1>::new(Config::default(), 42).unwrap();
    model.online.encoder[0] = [f32::MAX; 2];
    model.target_encoder[0] = [f32::MAX; 2];
    model.online.actions[0][0] = f32::MAX;
    assert_eq!(model.encode(&[2.0, 2.0]), Err(Error::NonFinite));
    assert_eq!(model.target(&[2.0, 2.0]), Err(Error::NonFinite));
    assert_eq!(model.predict(&[0.0; 3], &[2.0]), Err(Error::NonFinite));
}
