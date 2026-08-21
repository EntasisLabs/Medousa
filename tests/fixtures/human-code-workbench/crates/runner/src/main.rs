fn main() {
    println!("human-code-workbench fixture");
}

#[cfg(test)]
mod tests {
    #[test]
    fn runner_starts() {
        assert_eq!(2 + 2, 4);
    }
}
