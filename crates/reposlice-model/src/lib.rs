use reposlice_core::{Component, Entrypoint, ProjectModel};

pub fn find_component<'a>(model: &'a ProjectModel, component_id: &str) -> Option<&'a Component> {
    model
        .components
        .iter()
        .find(|component| component.id == component_id)
}

pub fn find_entrypoint<'a>(model: &'a ProjectModel, entrypoint_id: &str) -> Option<&'a Entrypoint> {
    model
        .entrypoints
        .iter()
        .find(|entrypoint| entrypoint.id == entrypoint_id)
}
