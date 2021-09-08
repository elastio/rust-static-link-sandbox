use color_eyre::{
    eyre::Context,
    eyre::{eyre, WrapErr},
    Report, Result,
};
use shiplift::{
    builder::{ImageFilter, ImageListOptions},
    rep::{ContainerCreateInfo, Image},
    ContainerOptions, Docker,
};
use tracing::*;

pub(crate) async fn connect_docker() -> Result<Docker> {
    let docker = shiplift::Docker::new();

    // Make sure it's working
    let version = docker.version().await?;

    debug!(?version, "Connected to Docker daemon");

    Ok(docker)
}

pub(crate) async fn get_image_by_label(docker: &Docker, label: &str) -> Result<Image> {
    let options = ImageListOptions::builder().build();

    let images = docker.images().list(&options).await?;

    // Find an image that has a repo tag `elastio:$label`
    let repo_tag = format!("elastio:{}", label);

    if let Some(image) = images.into_iter().find(|image| {
        image
            .repo_tags
            .as_ref()
            .map(|tags| tags.iter().any(|tag| tag == &repo_tag))
            .unwrap_or(false)
    }) {
        debug!(label, image_id = %image.id, "Found image by label");
        Ok(image)
    } else {
        Err(eyre!("No docker image found with label '{}'; did you run the `build-docker-images.sh` script?", label))
    }
}
