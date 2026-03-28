== Installation

You can run Lince as an executable, here are the #link("https://github.com/lince-social/lince/tags")[releases].
Pick the latest one for your machine and operating system. After the download, unzip it and run the binary:

```bash
./lince
```

Or you can install from the crates.io repository with:
```bash
cargo install lince
```

If you prefer to compile the source code, the easiest way is to download the repo and run:

```bash
cargo run
```

If you want local authentication enabled during setup, create or edit `~/.config/lince/lince.toml` and set:

```toml
[auth]
enabled = true
```

When auth is enabled, Lince will require an admin user to exist. If none exists yet, run `lince` once in an interactive terminal so it can prompt you to create the initial admin account.

If you don't have Cargo installed, you can install it with #link("https://www.rust-lang.org/tools/install")[Rustup].
It comes with Rust and its toolchain. If `cargo` isn’t in your `PATH`, add it to your shell configuration (`~/.bashrc` or `~/.zshrc`)
so you don’t have to type the full path every time (i.e., `~/.cargo/bin/{program}`). Run this in your terminal:

```bash
echo 'export PATH="$HOME/.cargo/bin:$PATH"' >> ~/.bashrc
echo 'export PATH="$HOME/.cargo/bin:$PATH"' >> ~/.zshrc
```

This uses #link("https://typst.app/")[Typst] to generate the documentation.
One way of running the project is with Mise, the command runner that installs the programs you need to run. To get Mise, run:
```bash
curl https://mise.run | sh
```
Then run `mise docs` for documentation.

*Lince Config*
Whenever you start Lince, it checks if a `lince.toml` file exists. It is in the config directory of your system, plus the lince/ directory.

Properties that may be in there are:

- `auth.enabled`: turns local authentication on or off.
- `auth.secret`: the secret used to sign the local auth session. Lince can generate it for you on first run.

Example:

```toml
[auth]
enabled = true
```

Bucket storage is not configured in `lince.toml`. It lives in the `configuration` table inside the app database, in the row with `quantity = 1`.

To configure a bucket, set these fields there:

- `bucket_enabled`: set to `1` to enable bucket storage.
- `bucket_username`: the access key / username for the bucket service.
- `bucket_password`: the secret key / password for the bucket service.
- `bucket_uri`: the bucket endpoint, such as `http://localhost:9000` for a local S3-compatible server.
- `bucket_name`: optional bucket name. If omitted, Lince uses `lince`.
- `bucket_region`: optional region. If omitted, Lince uses `us-east-1`.

If you are editing the database manually, update the active configuration row so it looks like this:

```sql
UPDATE configuration
SET bucket_enabled = 1,
    bucket_username = 'ACCESS_KEY',
    bucket_password = 'SECRET_KEY',
    bucket_uri = 'http://localhost:9000',
    bucket_name = 'lince',
    bucket_region = 'us-east-1'
WHERE quantity = 1;
```
