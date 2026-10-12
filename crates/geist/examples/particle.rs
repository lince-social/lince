use geist::{Config, Transition, WorldModel};

fn trajectories(first: usize, count: usize) -> Vec<Transition<2, 1>> {
    let mut transitions = Vec::with_capacity(count * 16);
    for trajectory in first..first + count {
        let phase = trajectory as f32 * 0.73;
        let mut state = [phase.sin() * 0.7, phase.cos() * 0.3];
        for step in 0..16 {
            let force = (phase + step as f32 * 0.31).sin() * 0.5;
            let velocity = 0.95 * state[1] - 0.1 * state[0] + 0.1 * force;
            let next = [state[0] + 0.1 * velocity, velocity];
            transitions.push(Transition {
                observation: state,
                action: [force],
                next_observation: next,
            });
            state = next;
        }
    }
    transitions
}

fn main() -> Result<(), geist::Error> {
    let training = trajectories(0, 16);
    let held_out = trajectories(16, 8);
    let mut model = WorldModel::<2, 8, 1>::new(Config::default(), 42)?;
    for step in 0..=2000 {
        if step % 500 == 0 {
            let train = model.evaluate(&training)?;
            let held = model.evaluate(&held_out)?;
            println!(
                "step={step} training_mse={:.6} held_out_mse={:.6} latent_std={:.4} variance_penalty={:.4}",
                train.prediction_loss, held.prediction_loss, held.latent_std, held.variance_loss
            );
        }
        if step < 2000 {
            model.train_step(&training)?;
        }
    }
    let rollout = model.rollout(&[0.3, 0.0], &[[0.2]; 8])?;
    println!("eight_step_predicted_latent={:?}", rollout.last().unwrap());
    Ok(())
}
