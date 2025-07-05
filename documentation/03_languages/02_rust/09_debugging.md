# Debugging

### 1. Ergonomics

To observe values during debugging, you can use:

```rust
println!("User: {}", user);
```

However, if a message alongside the value isn't necessary, it's easier to write:

```rust
dbg!(&user);
```

Example:

```rust
#[derive(Debug)]
struct User {
    name: String,
}

fn main() {
    let user = User {
        name: "Chose".to_string(),
    };

    dbg!(&user);
    println!("{user:?}");
    println!("User's name is: {}", user.name)
}
```

You can run the snippet, and it will show only the 'println!' lines, not the 'dbg!'. The real stdout output would be this:

```bash
 [src/main.rs:11:5] &user = User {
     name: "Chose",
 }
 User { name: "Chose" }
 User's name is: Chose
```

We can see that the `dbg!` macro outputs more information with less effort. Here's a great example: [link](https://edgl.dev/blog/rust-dbg-macro/).
