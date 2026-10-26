

# Geist experiments and Rust stack

Geist starts as a small handwritten world model so its learning rules can be inspected and changed. It currently learns to predict the representation of the next numeric observation from the current observation and an optional action. The Rust library has no runtime dependencies. It is a CPU reference for experiments, with no GPU acceleration or application integration yet.

## What JEPA means

- [ ] Being able to generate instantly a representation of something you want in any part of the Space, using Fiote (JEPA model?). 

A world model learns how an environment changes. A joint embedding predictive architecture, or JEPA, predicts a representation of an observation rather than reconstructing every detail of that observation. A representation is a vector of learned numbers, also called a latent or embedding.

For example, an observation could contain a particle's position and velocity. An action could be an applied force. The encoder maps the observation into a latent vector. The predictor takes that vector and the action and predicts the next latent vector. The target encoder supplies a representation of the actual next observation for comparison.

The central research question is what the representation preserves. Low prediction error alone is insufficient: an encoder that always returns the same vector makes prediction trivial and learns nothing useful. This is representation collapse.

[I-JEPA](https://github.com/facebookresearch/ijepa) predicts representations of hidden image regions. [V-JEPA 2](https://github.com/facebookresearch/vjepa2) includes learning from video and an action-conditioned world model. They provide reference implementations and experimental recipes; Geist does not reproduce their transformer architectures or training pipelines.

## The handwritten implementation

The public model type is `WorldModel<O, L, A>`, where `O` is the observation dimension, `L` is the latent dimension, and `A` is the action dimension. `A = 0` supports experiments without actions. Fixed arrays make dimensions explicit and keep this initial implementation easy to inspect. Large models will need a different storage and compute design.

The online encoder computes `z = tanh(W x + b)`. The predictor computes `prediction = tanh(D z + U action + c)`. The target encoder has its own weights and computes the representation of the next observation. Training uses the mean squared difference between prediction and target, plus a configurable penalty when the batch standard deviation of an online latent coordinate falls below a chosen floor.

Gradients are derived by hand. Only the online encoder and predictor receive gradients. Global gradient clipping bounds the update, and stochastic gradient descent changes the online parameters. After the update, the target encoder moves toward the new online encoder with `target = momentum * target + (1 - momentum) * online`. This is an exponential moving average, or EMA. Momentum of one freezes the target; momentum of zero copies the online encoder after every step.

The variance penalty is an experimental addition to encourage varied embeddings. It is not a guarantee against collapse, and it does not force different latent coordinates to contain different information. `latent_std` exposes a batch diagnostic. Evaluation and training metrics describe the model before the update; the reported standard deviation includes a small epsilon for numerical stability.

`encode` produces an online representation, `target` produces a teacher representation, `predict` advances a latent by one action, and `rollout` repeatedly applies the predictor. Rollout returns predicted latents after each action. It does not reconstruct observations or choose actions. `train_step` accepts at least two transitions; `evaluate` reports the same objective without changing the model.

The tests compare every online parameter gradient with a numerical finite difference while holding target parameters fixed. They also check EMA arithmetic, learning against a frozen teacher, repeatable initialization, action dependence, unconditioned prediction, and rejected batches leaving parameters unchanged. These checks establish implementation correctness for small examples, not useful world understanding.

## Rust crates worth evaluating

These are established candidates across the main layers of a Rust world-model stack. They are alternatives to evaluate, not dependencies already installed in Geist. Repository main branches can differ from published crates; choose released versions and features when adding a dependency.

| Layer | Crates and primary sources | Role and tradeoff |
| --- | --- | --- |
| Training framework | [burn](https://github.com/tracel-ai/burn) | Tensor operations, autodiff, model modules, optimizers and accelerated backends. My first framework candidate if handwritten training becomes too limiting. |
| Small tensor framework | [candle-core and candle-nn](https://github.com/huggingface/candle) | Tensors, training and custom operations with CPU and GPU support. Worth comparing when a smaller tensor API suits the experiment. |
| Shape checked learning | [dfdx](https://github.com/chelsea0x3b/dfdx) | Compile-time tensor shape checking and neural networks. Evaluate operation coverage and current maintenance before committing. |
| PyTorch access | [tch](https://github.com/LaurentMazare/tch-rs) | Rust bindings to the C++ PyTorch API. Useful for comparisons and existing operations, but requires LibTorch and gives less ownership of the compute implementation. |
| Portable custom kernels | [cubecl](https://github.com/tracel-ai/cubecl) | Write compute kernels in a Rust extension targeting multiple runtimes. My first candidate for keeping the training algorithm handwritten while moving heavy computation to accelerators. |
| NVIDIA compute | [cudarc](https://github.com/chelsea0x3b/cudarc) | CUDA driver and library wrappers, including cuBLAS, cuBLASLt and NCCL. Suitable when choosing NVIDIA and accepting responsibility for buffers, scheduling and gradients. |
| Portable GPU API | [wgpu](https://github.com/gfx-rs/wgpu) | GPU resource management and compute dispatch. More direct control than a tensor framework, with more kernel and scheduling work. |
| Vulkan API | [ash](https://github.com/ash-rs/ash) | Vulkan bindings. Consider only when Vulkan-specific control justifies the additional implementation complexity. |
| CPU linear algebra | [faer](https://github.com/sarah-quinones/faer-rs), [ndarray](https://github.com/rust-ndarray/ndarray) | Matrix computation or multidimensional array storage and views. Useful before building a full tensor engine; neither supplies a JEPA training recipe. |
| CPU concurrency | [rayon](https://github.com/rayon-rs/rayon) | Parallel work over CPU data. Choose thread ownership carefully when matrix libraries also parallelize. |
| Checkpoints | [safetensors](https://github.com/safetensors/safetensors) | Tensor serialization. A complete resumable experiment must also save architecture, optimizer state, random state and step counters. |
| Dataset storage | [arrow and parquet](https://github.com/apache/arrow-rs) | Columnar data and file storage for larger transition datasets. Small initial experiments can use generated arrays. |

The suggested progression is handwritten CPU code, then measured CPU matrix acceleration, then CubeCL or cudarc once a workload and device are known. Burn and Candle remain useful comparison implementations. Owning the model and training loop does not require writing every matrix multiplication yourself. Existing hardware libraries can provide strong performance while leaving architecture, memory policy and objectives under your control.

No crate can establish maximum performance in advance. Compare representative batch sizes and model dimensions, including data transfer, allocation, forward pass, backward pass and optimizer updates. GPU kernel timing requires device synchronization or device events; timing only dispatch measures the wrong thing.

## Experiments in sequence

1. Generate a small environment with known dynamics, such as a particle moving under an applied force. Split entire trajectories into training and held-out sets so neighboring transitions do not leak across the split. Normalize observation and action scales.
2. Train with a frozen target encoder first. This isolates whether the handwritten optimizer and predictor can learn a fixed objective. The existing learning test covers a small instance of this experiment.
3. Enable EMA and compare several momenta. Track held-out prediction loss, latent standard deviations and how well a simple probe recovers position and velocity. A probe is a small supervised model trained on frozen embeddings; it measures retained information.
4. Disable the variance penalty, then vary its weight and floor. Look for collapse, redundant latent coordinates and sensitivity to random seeds. Compare against constant predictions and an explicit next-state regression model.
5. Test rollouts at increasing horizons. A low one-step loss does not establish stable long-term dynamics. Use held-out trajectories and report errors per horizon.
6. Add one architectural change at a time: a deeper encoder, residual dynamics, a history window, a stochastic predictor, or several time scales. Compare each change with the same data splits and seed set.
7. Add checkpointing and a reusable experiment runner before longer runs. Save configuration, seeds, data identity and metrics so experiments can be repeated.
8. Profile the CPU implementation. Replace the measured bottleneck with batched matrix operations, then evaluate a GPU implementation against the reference gradients and outputs. Mixed precision and fused kernels should follow correctness checks.
9. Introduce images, video or other modalities only when the experiment requires them. They add preprocessing, tokenization, masking and substantially larger compute requirements.

## Verification commands

Use `cargo check -p geist --all-targets` for compilation and `nix shell nixpkgs#bash -c cargo test -p geist --lib` for the learning and correctness tests. The repository also supports test forwarding through `cargo xtask`.

Run `nix shell nixpkgs#bash -c cargo run -p geist --example particle` for the first experiment. It generates trajectories of a damped particle driven by a changing force, holds out separate trajectories, trains for 2,000 steps and prints training loss, held-out loss, latent variation and an eight-step latent rollout. You can change the dynamics in `examples/particle.rs` and the objective in `src/lib.rs` independently. The target encoder changes during this run, so falling latent loss alone does not establish retained physical information; probe and rollout evaluations are the next experiments.

