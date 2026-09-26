//! Agent skills built into the binary, so that an agent reads the instructions for the Iris it
//! runs rather than a copy that may be out of date.

pub struct Skill {
    pub name: &'static str,
    content: &'static str,
}

pub const SKILLS: &[Skill] =
    &[Skill { name: "watch", content: include_str!("../skills/watch/SKILL.md") }];

impl Skill {
    pub fn find(name: &str) -> Option<&'static Skill> {
        SKILLS.iter().find(|skill| skill.name == name)
    }

    pub fn content(&self) -> &'static str {
        self.content
    }

    /// The `description` field of the skill's front matter.
    pub fn description(&self) -> &'static str {
        let description = self.content.lines().find_map(|line| line.strip_prefix("description: "));
        description.expect("invariant violated: a built-in skill has no description")
    }
}
