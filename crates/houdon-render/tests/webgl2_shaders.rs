//! Every shader must translate to GLSL ES 3.0 for WebGL2 (ADR 0001). The
//! render tests run on Vulkan, which accepts things WebGL2 does not.

use naga::back::glsl;
use naga::valid::{Capabilities, ValidationFlags, Validator};

fn assert_translates_for_webgl2(
    name: &str,
    source: &str,
    entry_points: &[(naga::ShaderStage, &str)],
) {
    let module = naga::front::wgsl::parse_str(source).unwrap_or_else(|e| panic!("{name}: {e}"));
    let info = Validator::new(ValidationFlags::all(), Capabilities::empty())
        .validate(&module)
        .unwrap_or_else(|e| panic!("{name}: {e:?}"));
    let options = glsl::Options {
        version: glsl::Version::Embedded {
            version: 300,
            is_webgl: true,
        },
        ..glsl::Options::default()
    };
    for &(shader_stage, entry_point) in entry_points {
        let pipeline = glsl::PipelineOptions {
            shader_stage,
            entry_point: entry_point.to_owned(),
            multiview: None,
        };
        let mut out = String::new();
        glsl::Writer::new(
            &mut out,
            &module,
            &info,
            &options,
            &pipeline,
            naga::proc::BoundsCheckPolicies::default(),
        )
        .and_then(|mut writer| writer.write())
        .unwrap_or_else(|e| panic!("{name} {entry_point} does not translate for WebGL2: {e}"));
    }
}

#[test]
fn the_scene_shader_runs_on_webgl2() {
    assert_translates_for_webgl2(
        "shader.wgsl",
        include_str!("../src/shader.wgsl"),
        &[
            (naga::ShaderStage::Vertex, "vs_main"),
            (naga::ShaderStage::Fragment, "fs_main"),
        ],
    );
}

#[test]
fn the_shadow_shader_runs_on_webgl2() {
    assert_translates_for_webgl2(
        "shadow.wgsl",
        include_str!("../src/shadow.wgsl"),
        &[
            (naga::ShaderStage::Vertex, "vs_shadow"),
            (naga::ShaderStage::Fragment, "fs_shadow"),
        ],
    );
}

#[test]
fn the_composite_shader_runs_on_webgl2() {
    assert_translates_for_webgl2(
        "composite.wgsl",
        include_str!("../src/composite.wgsl"),
        &[
            (naga::ShaderStage::Vertex, "vs_composite"),
            (naga::ShaderStage::Fragment, "fs_composite"),
        ],
    );
}
