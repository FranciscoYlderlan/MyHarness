//! Application layer: use cases from the functional contract (`03-casos-de-uso.md`, kept in the project vault until final acceptance). Empty for now.

#[cfg(test)]
mod tests {
    #[test]
    fn application_module_compiles() {
        assert!(!env!("CARGO_PKG_NAME").is_empty());
    }
}
