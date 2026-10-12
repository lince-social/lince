use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Config {
    pub learning_rate: f32,
    pub target_momentum: f32,
    pub variance_weight: f32,
    pub variance_floor: f32,
    pub gradient_clip: f32,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            learning_rate: 0.01,
            target_momentum: 0.99,
            variance_weight: 1.0,
            variance_floor: 0.2,
            gradient_clip: 1.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    InvalidConfig,
    InvalidDimensions,
    BatchTooSmall,
    NonFinite,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InvalidConfig => "invalid training configuration",
            Self::InvalidDimensions => "observation and latent dimensions must be positive",
            Self::BatchTooSmall => "training requires at least two transitions",
            Self::NonFinite => "inputs or computation contain non-finite values",
        })
    }
}

impl std::error::Error for Error {}

#[derive(Clone, Copy, Debug)]
pub struct Transition<const O: usize, const A: usize> {
    pub observation: [f32; O],
    pub action: [f32; A],
    pub next_observation: [f32; O],
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Metrics {
    pub prediction_loss: f32,
    pub variance_loss: f32,
    pub latent_std: f32,
    pub gradient_norm: f32,
}

#[derive(Clone, Debug)]
struct Parameters<const O: usize, const L: usize, const A: usize> {
    encoder: [[f32; O]; L],
    encoder_bias: [f32; L],
    dynamics: [[f32; L]; L],
    actions: [[f32; A]; L],
    predictor_bias: [f32; L],
}

impl<const O: usize, const L: usize, const A: usize> Parameters<O, L, A> {
    fn zero() -> Self {
        Self {
            encoder: [[0.0; O]; L],
            encoder_bias: [0.0; L],
            dynamics: [[0.0; L]; L],
            actions: [[0.0; A]; L],
            predictor_bias: [0.0; L],
        }
    }

    fn values(&self) -> impl Iterator<Item = &f32> {
        self.encoder
            .iter()
            .flatten()
            .chain(self.encoder_bias.iter())
            .chain(self.dynamics.iter().flatten())
            .chain(self.actions.iter().flatten())
            .chain(self.predictor_bias.iter())
    }

    fn values_mut(&mut self) -> impl Iterator<Item = &mut f32> {
        self.encoder
            .iter_mut()
            .flatten()
            .chain(self.encoder_bias.iter_mut())
            .chain(self.dynamics.iter_mut().flatten())
            .chain(self.actions.iter_mut().flatten())
            .chain(self.predictor_bias.iter_mut())
    }
}

#[derive(Clone, Debug)]
pub struct WorldModel<const O: usize, const L: usize, const A: usize> {
    config: Config,
    online: Parameters<O, L, A>,
    target_encoder: [[f32; O]; L],
    target_bias: [f32; L],
}

fn finite(values: impl IntoIterator<Item = f32>) -> Result<(), Error> {
    if values.into_iter().all(f32::is_finite) {
        Ok(())
    } else {
        Err(Error::NonFinite)
    }
}

fn encode<const O: usize, const L: usize>(
    weights: &[[f32; O]; L],
    bias: &[f32; L],
    observation: &[f32; O],
) -> Result<[f32; L], Error> {
    let affine: [f32; L] = std::array::from_fn(|i| {
        bias[i]
            + weights[i]
                .iter()
                .zip(observation)
                .map(|(w, x)| w * x)
                .sum::<f32>()
    });
    finite(affine)?;
    Ok(affine.map(f32::tanh))
}

impl<const O: usize, const L: usize, const A: usize> WorldModel<O, L, A> {
    pub fn new(config: Config, seed: u64) -> Result<Self, Error> {
        if O == 0 || L == 0 {
            return Err(Error::InvalidDimensions);
        }
        if !config.learning_rate.is_finite()
            || config.learning_rate <= 0.0
            || !config.target_momentum.is_finite()
            || !(0.0..=1.0).contains(&config.target_momentum)
            || !config.variance_weight.is_finite()
            || config.variance_weight < 0.0
            || !config.variance_floor.is_finite()
            || !(0.0..=1.0).contains(&config.variance_floor)
            || !config.gradient_clip.is_finite()
            || config.gradient_clip <= 0.0
        {
            return Err(Error::InvalidConfig);
        }
        let mut state = seed;
        let mut random = |fan_in: usize| {
            state = state.wrapping_add(0x9e3779b97f4a7c15);
            let mut z = state;
            z = (z ^ (z >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94d049bb133111eb);
            z ^= z >> 31;
            let unit = (z >> 40) as f32 / 16_777_216.0;
            (2.0 * unit - 1.0) * (3.0 / fan_in as f32).sqrt()
        };
        let mut online = Parameters::zero();
        for value in online.encoder.iter_mut().flatten() {
            *value = random(O);
        }
        for value in online.dynamics.iter_mut().flatten() {
            *value = random(L + A);
        }
        for value in online.actions.iter_mut().flatten() {
            *value = random(L + A);
        }
        Ok(Self {
            config,
            target_encoder: online.encoder,
            target_bias: online.encoder_bias,
            online,
        })
    }

    pub fn encode(&self, observation: &[f32; O]) -> Result<[f32; L], Error> {
        finite(observation.iter().copied())?;
        let latent = encode(&self.online.encoder, &self.online.encoder_bias, observation)?;
        finite(latent)?;
        Ok(latent)
    }

    pub fn target(&self, observation: &[f32; O]) -> Result<[f32; L], Error> {
        finite(observation.iter().copied())?;
        let latent = encode(&self.target_encoder, &self.target_bias, observation)?;
        finite(latent)?;
        Ok(latent)
    }

    pub fn predict(&self, latent: &[f32; L], action: &[f32; A]) -> Result<[f32; L], Error> {
        finite(latent.iter().chain(action).copied())?;
        let affine: [f32; L] = std::array::from_fn(|i| {
            self.online.predictor_bias[i]
                + self.online.dynamics[i]
                    .iter()
                    .zip(latent)
                    .map(|(w, x)| w * x)
                    .sum::<f32>()
                + self.online.actions[i]
                    .iter()
                    .zip(action)
                    .map(|(w, x)| w * x)
                    .sum::<f32>()
        });
        finite(affine)?;
        Ok(affine.map(f32::tanh))
    }

    pub fn rollout(
        &self,
        observation: &[f32; O],
        actions: &[[f32; A]],
    ) -> Result<Vec<[f32; L]>, Error> {
        let mut latent = self.encode(observation)?;
        actions
            .iter()
            .map(|action| {
                latent = self.predict(&latent, action)?;
                Ok(latent)
            })
            .collect()
    }

    fn objective(
        &self,
        batch: &[Transition<O, A>],
    ) -> Result<(Metrics, Parameters<O, L, A>), Error> {
        if batch.len() < 2 {
            return Err(Error::BatchTooSmall);
        }
        let count = batch.len() as f32;
        let scale = 1.0 / (count * L as f32);
        let mut latents = Vec::with_capacity(batch.len());
        let mut targets = Vec::with_capacity(batch.len());
        let mut predictions = Vec::with_capacity(batch.len());
        for transition in batch {
            let latent = self.encode(&transition.observation)?;
            predictions.push(self.predict(&latent, &transition.action)?);
            latents.push(latent);
            targets.push(self.target(&transition.next_observation)?);
        }
        let means: [f32; L] =
            std::array::from_fn(|i| latents.iter().map(|z| z[i]).sum::<f32>() / count);
        let stds: [f32; L] = std::array::from_fn(|i| {
            (latents
                .iter()
                .map(|z| (z[i] - means[i]).powi(2))
                .sum::<f32>()
                / count
                + 1e-4)
                .sqrt()
        });
        let variance_loss = stds
            .iter()
            .map(|s| (self.config.variance_floor - s).max(0.0))
            .sum::<f32>()
            / L as f32;
        let mut prediction_loss = 0.0;
        let mut gradient = Parameters::zero();
        for (n, transition) in batch.iter().enumerate() {
            let latent = &latents[n];
            let mut latent_gradient = [0.0; L];
            for i in 0..L {
                let difference = predictions[n][i] - targets[n][i];
                prediction_loss += difference.powi(2) * scale;
                let derivative = 2.0 * difference * scale * (1.0 - predictions[n][i].powi(2));
                gradient.predictor_bias[i] += derivative;
                for j in 0..L {
                    gradient.dynamics[i][j] += derivative * latent[j];
                    latent_gradient[j] += derivative * self.online.dynamics[i][j];
                }
                for j in 0..A {
                    gradient.actions[i][j] += derivative * transition.action[j];
                }
            }
            for i in 0..L {
                if stds[i] < self.config.variance_floor {
                    latent_gradient[i] -=
                        self.config.variance_weight * scale * (latent[i] - means[i]) / stds[i];
                }
                let derivative = latent_gradient[i] * (1.0 - latent[i].powi(2));
                gradient.encoder_bias[i] += derivative;
                for j in 0..O {
                    gradient.encoder[i][j] += derivative * transition.observation[j];
                }
            }
        }
        finite(gradient.values().copied())?;
        let gradient_norm = gradient
            .values()
            .map(|g| f64::from(*g).powi(2))
            .sum::<f64>()
            .sqrt() as f32;
        let metrics = Metrics {
            prediction_loss,
            variance_loss,
            latent_std: stds.iter().sum::<f32>() / L as f32,
            gradient_norm,
        };
        finite([
            metrics.prediction_loss,
            metrics.variance_loss,
            metrics.latent_std,
            metrics.gradient_norm,
        ])?;
        Ok((metrics, gradient))
    }

    pub fn evaluate(&self, batch: &[Transition<O, A>]) -> Result<Metrics, Error> {
        self.objective(batch).map(|(metrics, _)| metrics)
    }

    pub fn train_step(&mut self, batch: &[Transition<O, A>]) -> Result<Metrics, Error> {
        let (metrics, gradient) = self.objective(batch)?;
        let clip =
            (self.config.gradient_clip / metrics.gradient_norm.max(f32::MIN_POSITIVE)).min(1.0);
        let mut next = self.online.clone();
        for (value, derivative) in next.values_mut().zip(gradient.values()) {
            *value -= self.config.learning_rate * clip * derivative;
        }
        finite(next.values().copied())?;
        let momentum = self.config.target_momentum;
        for (target, value) in self
            .target_encoder
            .iter_mut()
            .flatten()
            .chain(self.target_bias.iter_mut())
            .zip(
                next.encoder
                    .iter()
                    .flatten()
                    .chain(next.encoder_bias.iter()),
            )
        {
            *target = momentum * *target + (1.0 - momentum) * value;
        }
        self.online = next;
        Ok(metrics)
    }
}

#[cfg(test)]
mod tests;
