@group(0) @binding(0) var cells: texture_storage_2d<r32uint, read>;
@group(0) @binding(1) var cells_out: texture_storage_2d<r32uint, write>;
@group(0) @binding(2) var screen: texture_storage_2d<rgba8unorm, write>;

const NEIGHBORS: array<vec2<i32>, 8> = array(
    vec2(-1, -1),
    vec2( 0, -1),
    vec2( 1, -1),
    vec2(-1,  0),
    vec2( 1,  0),
    vec2(-1,  1),
    vec2( 0,  1),
    vec2( 1,  1),
);

@compute @workgroup_size(16, 16)
fn update(@builtin(global_invocation_id) id: vec3<u32>) {
    let dims = textureDimensions(cells);
    let size = vec2<i32>(dims);
    let pos = vec2<i32>(i32(id.x), i32(id.y));

    if all(pos < size) {
        var count: u32 = 0;

        var load_pos = pos + NEIGHBORS[0];
        if all(load_pos >= vec2(0) && load_pos != size) {
            count += textureLoad(cells, load_pos).r; 
        }
        load_pos = pos + NEIGHBORS[1];
        if all(load_pos >= vec2(0) && load_pos != size) {
            count += textureLoad(cells, load_pos).r; 
        }
        load_pos = pos + NEIGHBORS[2];
        if all(load_pos >= vec2(0) && load_pos != size) {
            count += textureLoad(cells, load_pos).r; 
        }
        load_pos = pos + NEIGHBORS[3];
        if all(load_pos >= vec2(0) && load_pos != size) {
            count += textureLoad(cells, load_pos).r; 
        }
        load_pos = pos + NEIGHBORS[4];
        if all(load_pos >= vec2(0) && load_pos != size) {
            count += textureLoad(cells, load_pos).r; 
        }
        load_pos = pos + NEIGHBORS[5];
        if all(load_pos >= vec2(0) && load_pos != size) {
            count += textureLoad(cells, load_pos).r; 
        }
        load_pos = pos + NEIGHBORS[6];
        if all(load_pos >= vec2(0) && load_pos != size) {
            count += textureLoad(cells, load_pos).r; 
        }
        load_pos = pos + NEIGHBORS[7];
        if all(load_pos >= vec2(0) && load_pos != size) {
            count += textureLoad(cells, load_pos).r; 
        }

        let was_alive: u32 = textureLoad(cells, pos).r;

        var alive: u32;
        var colour: vec3<f32>;
        if count == 3u || (was_alive == 1u && count == 2u) {
            alive = 1u;
            colour = vec3(1.0);
        } else {
            alive = 0u;
            colour = vec3(0.0);
        }
        
        textureStore(cells_out, pos, vec4(alive, 0, 0, 0));
        textureStore(screen, pos, vec4(colour, 1.0));
    }
}
