//! Objects in the picture: empty scenes, figures, several objects, selection.

use crate::*;

#[test]
fn an_empty_scene_renders_only_background() {
    let Some(mut renderer) = renderer() else {
        return;
    };
    let mut scene = Scene::new();
    scene.room.visible = false;
    let image = render(&mut renderer, &scene);
    assert_eq!(
        background_pixels(&image, scene.room.backdrop_srgb()),
        (SIZE * SIZE) as usize
    );
}

#[test]
fn a_figure_covers_part_of_the_picture() {
    let Some(mut renderer) = renderer() else {
        return;
    };
    let scene = figure_scene();
    let image = render(&mut renderer, &scene);
    let covered = (SIZE * SIZE) as usize - background_pixels(&image, scene.room.backdrop_srgb());
    // A framed standing figure covers a narrow vertical band of the square.
    assert!(covered > 100, "only {covered} pixels show the figure");
    assert!(
        covered < (SIZE * SIZE / 2) as usize,
        "{covered} pixels show the figure"
    );
    // The centre of the picture is on the figure's body.
    let centre = image.pixel(SIZE / 2, SIZE / 2);
    assert_ne!(&centre[..3], &scene.room.backdrop_srgb()[..]);
}

pub(crate) fn two_figures_apart() -> (Scene, houdon_core::ObjectId, houdon_core::ObjectId) {
    let mut model = Model::from_gltf_slice("male", MALE).unwrap();
    model.normalize_to_height(DEFAULT_FIGURE_HEIGHT);
    let mut scene = Scene::new();
    scene.room.visible = false;
    let id = scene.add_model(model);
    let left = scene.add_object(id).unwrap();
    let right = scene.add_object(id).unwrap();
    scene.frame_camera(1.0);
    (scene, left, right)
}

pub(crate) fn column_has_figure(image: &Image, x: u32, background: [u8; 3]) -> bool {
    (0..image.height).any(|y| {
        let p = image.pixel(x, y);
        p[..3]
            .iter()
            .zip(background)
            .any(|(a, b)| a.abs_diff(b) > 1)
    })
}

#[test]
fn two_objects_of_one_model_stand_side_by_side() {
    let Some(mut renderer) = renderer() else {
        return;
    };
    let (scene, _, _) = two_figures_apart();
    let image = render(&mut renderer, &scene);
    let bg = scene.room.backdrop_srgb();
    let figure_columns: Vec<bool> = (0..SIZE)
        .map(|x| column_has_figure(&image, x, bg))
        .collect();
    // Two separate bands of figure columns with background between them.
    let bands = figure_columns.windows(2).filter(|w| !w[0] && w[1]).count()
        + usize::from(figure_columns[0]);
    assert_eq!(bands, 2, "columns with figure: {figure_columns:?}");
}

#[test]
fn the_selected_object_is_highlighted() {
    let Some(mut renderer) = renderer() else {
        return;
    };
    let (mut scene, left, _) = two_figures_apart();
    let plain = render(&mut renderer, &scene);
    scene.select(Some(left));
    let selected = render(&mut renderer, &scene);
    let half = SIZE / 2;
    let differs = |x0: u32, x1: u32| {
        (x0..x1).any(|x| (0..SIZE).any(|y| plain.pixel(x, y) != selected.pixel(x, y)))
    };
    assert!(differs(0, half), "the selected left figure looks different");
    assert!(!differs(half, SIZE), "the right figure is unchanged");
}
