struct Input {
    t: f32,
}

@group(0) @binding(0) var<uniform> input: Input;

struct VertexOutput {
    @location(0) colour: vec3<f32>,
    @builtin(position) position: vec4<f32>,
};

@vertex fn vertex(
    @location(0) v: vec3<f32>,
    @location(1) c: vec3<f32>
) -> VertexOutput {
    var out: VertexOutput;
    out.position = vec4<f32>(v.x * input.t, v.yz, 1.0);
    out.colour = c;
    return out;
}

@fragment fn frag(in: VertexOutput) -> @location(0) vec4<f32> {
    return vec4(in.colour, 1.0);
}

