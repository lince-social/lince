procs:
	cargo install mprocs --locked
	mprocs "bacon . --job clippy-all" "bacon . --job run"
