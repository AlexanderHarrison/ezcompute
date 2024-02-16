@vertex fn vertex(@location(0) pos: vec2<f32>) -> @builtin(position) vec4<f32> {
    return vec4(pos, 0.0, 1.0);
}

@fragment fn fragment(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
    return vec4(1.0);
}
