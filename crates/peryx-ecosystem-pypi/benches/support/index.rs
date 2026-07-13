use peryx_ecosystem_pypi::{Meta, ProjectList, ProjectListEntry};

pub fn index_list(projects: usize) -> ProjectList {
    ProjectList {
        meta: Meta::default(),
        projects: (0..projects)
            .map(|index| ProjectListEntry {
                name: format!("project-{index}"),
            })
            .collect(),
    }
}
