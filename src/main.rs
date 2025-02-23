fn main() {
    let x = 3;
    let mut s1 = String::from("hello");

    if x >= 3 {
        let foo = &mut s1;
        foo.push_str(", word");
        println!("{foo}")
    } else {
        let foo = &mut s1;
        foo.push_str(", onsdoicdn");
        println!("{foo}")
    }
}

// fn takeaway(s: &mut String) {
//     s.push_str(", world");
//     println!("{s}")
// }
