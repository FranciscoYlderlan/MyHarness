//! Application layer: use cases from `docs/contrato/03-casos-de-uso.md`. Empty for now.

#[cfg(test)]
mod tests {
    #[test]
    fn application_module_compiles() {
        assert!(!env!("CARGO_PKG_NAME").is_empty());
    }
}
