use std::sync::LazyLock;

pub static KUBECTL: LazyLock<usage::Spec> = LazyLock::new(|| {
    include_str!("../../../usage/kubectl.usage.kdl")
        .parse()
        .expect("the bundled kubectl usage spec must be valid")
});

#[cfg(test)]
mod tests {
    use super::*;

    fn count(cmd: &usage::SpecCommand) -> usize {
        1 + cmd.subcommands.values().map(count).sum::<usize>()
    }

    #[test]
    fn parses_all_man_pages_as_usage_commands() {
        assert_eq!(KUBECTL.bin, "kubectl");
        assert_eq!(count(&KUBECTL.cmd), 106);
        assert!(KUBECTL.cmd.subcommands.contains_key("api-resources"));
        assert!(KUBECTL.cmd.subcommands["create"]
            .subcommands
            .contains_key("deployment"));
        let output = KUBECTL.cmd.subcommands["get"]
            .flags
            .iter()
            .find(|flag| flag.long.iter().any(|name| name == "output"))
            .unwrap();
        assert_eq!(output.short, vec!['o']);
        assert!(output
            .arg
            .as_ref()
            .unwrap()
            .choices
            .as_ref()
            .unwrap()
            .choices
            .contains(&"yaml".into()));
    }
}
