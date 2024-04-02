use ezcompute::*;
use glam::{i32::IVec3, f32::Mat4, f32::Vec3, f32::Quat};

macro_rules! repeat {
    ($n:expr, [ $($t:expr),+$(,)? ]) => {
        const _LEN: usize = [$($t,)].len();
        const _REP_COUNT: usize = $n / _LEN;
        const _LEFT: usize = $n % _LEN;
        [

        ]
    }
}

type ColourIdx = u16;

#[derive(Copy, Clone, Debug)]
pub enum Face {
    XNeg = 0,
    XPos = 1,
    YNeg = 2,
    YPos = 3,
    ZNeg = 4,
    ZPos = 5,
}

#[derive(Copy, Clone, Debug, bytemuck::NoUninit)]
#[repr(C)]
pub struct VoxelSurface {
    /// MUST be the negative-most ends of the surface rect
    pub inner_pos: u16,
    pub face: u8,
    pub extent: u8,
    pub chunk_idx: u16,
    pub colour_idx: ColourIdx,
}

#[derive(Copy, Clone, Debug, bytemuck::NoUninit)]
#[repr(C)]
pub struct ChunkRef {
    pub offset: IVec3,
    pub data_ref: ChunkDataRef,
}

#[derive(Copy, Clone, Debug)]
#[repr(C)]
pub struct Camera {
    // voxel based
    pub pos: Vec3,
    pub angle: f32,
}

impl Camera {
    pub fn move_dir(&mut self, dir: Vec3) {
        self.pos += Quat::from_rotation_y(-self.angle) * dir;
    }

    pub fn to_matrix(self) -> Mat4 {
        let perspective_mat = Mat4::perspective_lh(
            1.0, 
            1.0,
            0.1,
            32.0*10.0
        );

        let rotation_mat = Mat4::from_rotation_y(self.angle);
        let translation_mat = Mat4::from_translation(-self.pos);

        perspective_mat * (rotation_mat * translation_mat)
    }
}

pub type ChunkLayer = [ColourIdx; 32*32];
pub type ChunkLayerMask = [u32; 32];
pub const ZERO_LAYER_MASK: ChunkLayerMask = [0; 32];

#[derive(Clone, Debug)]
pub struct ChunkData {
    pub layers: Vec<ChunkLayer>,
    pub layer_masks: Vec<ChunkLayerMask>,
}

#[derive(Copy, Clone, Debug, bytemuck::NoUninit)]
#[repr(C)]
pub struct ChunkDataRef {
    pub idx: u16,
    pub layer_start: u8,
    pub layer_len: u8,
}
pub const ZERO_CHUNK_DATA_REF: ChunkDataRef = ChunkDataRef { idx: 0, layer_start: 0, layer_len: 0 };

#[derive(Copy, Clone, Debug)]
pub struct ChunkLayerRange<'a> {
    pub layers: &'a [ChunkLayer],
    pub layer_start: usize
}

#[derive(Copy, Clone, Debug)]
pub struct ChunkLayerMaskRange<'a> {
    pub layers: &'a [ChunkLayerMask],
    pub layer_start: usize
}

impl<'a> ChunkLayerMaskRange<'a> {
    fn in_mask_range(self, y_idx: usize) -> bool {
        let layer_start = self.layer_start;
        y_idx >= layer_start && y_idx < layer_start+self.layers.len()
    }

    pub fn y_range(self) -> std::ops::Range<usize> {
        self.layer_start..(self.layer_start+self.layers.len())
    }

    pub fn layer(self, y_idx: usize) -> &'a ChunkLayerMask {
        let layer_start = self.layer_start;

        if self.in_mask_range(y_idx) {
            &self.layers[y_idx - layer_start]
        } else {
            &ZERO_LAYER_MASK
        }
    }
}

impl<'a> ChunkLayerRange<'a> {
    /// voxel MUST be occupied
    pub fn texture_idx(self, inner_pos: u16) -> ColourIdx {
        let y_idx = inner_pos as usize >> 10;
        let layer_start = self.layer_start;
        let layer_idx = inner_pos & 0x3FF;
        self.layers[y_idx - layer_start]
            [layer_idx as usize] - 1
    }
}

impl ChunkDataRef {
    // range in ChunkData
    pub fn data_range(self) -> std::ops::Range<usize> {
        let idx = self.idx as usize;
        let len = self.layer_len as usize;

        idx..(idx+len)
    }
}

impl ChunkData {
    pub fn layer_masks(&self, range: ChunkDataRef) -> ChunkLayerMaskRange {
        let data_range = range.data_range();
        let layers = &self.layer_masks[data_range];
        ChunkLayerMaskRange {
            layers, 
            layer_start: range.layer_start as usize,
        }
    }

    pub fn layers(&self, range: ChunkDataRef) -> ChunkLayerRange {
        let data_range = range.data_range();
        let layers = &self.layers[data_range];
        ChunkLayerRange {
            layers, 
            layer_start: range.layer_start as usize,
        }
    }

    /// returns index
    pub fn add_layers(&mut self, layers: &[ChunkLayer]) -> u16 {
        let idx = self.layers.len() as _;

        for layer in layers.iter() {
            self.layer_masks.push([0u32; 32]);
            let last_idx = self.layer_masks.len()-1;
            let masks = &mut self.layer_masks[last_idx];

            for z in 0..32 {
                let row_slice = &layer[(z << 5)..][..32];

                let mut mask = 0u32;
                for x in 0..32 {
                    if row_slice[x] != 0 {
                        mask ^= 1 << x;
                    }
                }

                masks[z] = mask;
            }
        }

        self.layers.extend_from_slice(layers);

        idx
    }
}

const fn pack_chunk_pos(x: usize, y: usize, z: usize) -> u16 {
    ((y << 10) ^ (z << 5) ^ x) as u16
}

fn find_chunk(chunk_refs: &[ChunkRef], offset: IVec3) -> Option<ChunkDataRef> {
    for i in 0..chunk_refs.len() {
        let pos = chunk_refs[i];
        if pos.offset == offset {
            return Some(pos.data_ref)
        }
    }

    None
}

#[derive(Copy, Clone, Debug)]
pub struct ChunkToAdd<'a> {
    pub layers: &'a [ChunkLayer],
    pub layer_start: u8,
}

pub struct Chunks {
    pub data: ChunkData,
    pub refs:  Vec<ChunkRef>,
}

impl Chunks {
    pub fn new() -> Self {
        Chunks {
            data: ChunkData { layers: Vec::new(), layer_masks: Vec::new() },
            refs: Vec::new(),
        }
    }

    /// does not unload existing chunks
    pub fn load(
        &mut self,
        chunk_positions: &[IVec3],
        chunks_to_add: &[ChunkToAdd],
    ) {
        assert_eq!(
            chunk_positions.len(),
            chunks_to_add.len(),
            "load_chunks: chunk_positions and chunks_to_add must have the same length",
        );
        for (pos, chunk) in chunk_positions.iter().zip(chunks_to_add) {
            let idx = self.data.add_layers(chunk.layers);

            let chunk_ref = ChunkRef {
                offset: *pos,
                data_ref: ChunkDataRef {
                    idx,
                    layer_start: chunk.layer_start,
                    layer_len: chunk.layers.len() as _,
                }
            };

            self.refs.push(chunk_ref);
        }
    }

    fn calculate_surfaces(&self) -> Vec<VoxelSurface> {
        let chunk_data = &self.data;
        let chunk_refs = &self.refs;
        let mut surfaces = Vec::new();
        
        let t = std::time::Instant::now();

        const VEC: Vec<u16> = Vec::new();
        let mut faces = [VEC; 6];
        for f in faces.iter_mut() { f.reserve(128) }

        let mut face_l_mat = [0u32; 32];
        let mut face_r_mat = [0u32; 32];

        for (chunk_num, chunk_ref) in chunk_refs.iter().copied().enumerate() {
            let chunk = chunk_ref.data_ref;
            let chunk_pos = chunk_ref.offset;
            let collision = chunk_data.layer_masks(chunk);
            let chunk_layers = chunk_data.layers(chunk);

            let neighbor_masks = [
                chunk_data.layer_masks(find_chunk(chunk_refs, IVec3::new(-1, 0, 0) + chunk_pos).unwrap_or(ZERO_CHUNK_DATA_REF)),
                chunk_data.layer_masks(find_chunk(chunk_refs, IVec3::new( 1, 0, 0) + chunk_pos).unwrap_or(ZERO_CHUNK_DATA_REF)),
                chunk_data.layer_masks(find_chunk(chunk_refs, IVec3::new(0, -1, 0) + chunk_pos).unwrap_or(ZERO_CHUNK_DATA_REF)),
                chunk_data.layer_masks(find_chunk(chunk_refs, IVec3::new(0,  1, 0) + chunk_pos).unwrap_or(ZERO_CHUNK_DATA_REF)),
                chunk_data.layer_masks(find_chunk(chunk_refs, IVec3::new(0, 0, -1) + chunk_pos).unwrap_or(ZERO_CHUNK_DATA_REF)),
                chunk_data.layer_masks(find_chunk(chunk_refs, IVec3::new(0, 0,  1) + chunk_pos).unwrap_or(ZERO_CHUNK_DATA_REF)),
            ];

            let mut layer_mask_d = neighbor_masks[2].layer(31);
            
            for (layer_i, layer) in collision.layers.iter().enumerate() {
                let y = collision.layer_start + layer_i;

                let layer_mask_u = if y != 31 {
                    collision.layer(y+1)
                } else {
                    neighbor_masks[3].layer(0)
                };

                let layer_mask = collision.layers[layer_i];
                let layer_mask_l = neighbor_masks[0].layer(y);
                let layer_mask_r = neighbor_masks[1].layer(y);

                for z in 0..32 {
                    let row = layer_mask[z];
                    if row == 0 { continue; }

                    let row_l = (layer_mask_l[z] >> 31) ^ (row << 1);
                    let row_r = (layer_mask_r[z] << 31) ^ (row >> 1);
                    let face_l = row & !row_l;
                    let face_r = row & !row_r;

                    face_l_mat[z] = face_l;
                    face_r_mat[z] = face_r;
                }

                // http://www.icodeguru.com/Embedded/Hacker's-Delight/048.htm
                fn transpose(mat: &mut [u32; 32]) {
                    let mut j: u32 = 16;
                    let mut m: u32 = 0x0000FFFF; 

                    while j != 0 {
                        let mut k: u32 = 0;
                        while k < 32 {
                            let kj = (k+j) as usize;
                            let ku = k as usize;
                            let t = (mat[ku] ^ (mat[kj] >> j)) & m; 
                            mat[ku] = mat[ku] ^ t; 
                            mat[kj] = mat[kj] ^ (t << j); 

                            k = (k + j + 1) & !j;
                        }

                        j >>= 1;
                        m ^= m << j;
                    }
                }

                transpose(&mut face_l_mat);
                transpose(&mut face_r_mat);
                
                fn add_run_lengths(
                    surfaces: &mut Vec<VoxelSurface>,
                    layers: ChunkLayerRange,
                    row: u32,
                    axis_shift: u32,
                    base: VoxelSurface,
                ) {
                    let mut face_start = row & !(row << 1);
                    let mut face_end = row & !(row >> 1);
                    while face_start != 0 {
                        let start = face_start.trailing_zeros();
                        let end = face_end.trailing_zeros();
                        face_start ^= 1 << start;
                        face_end ^= 1 << end;

                        let extent = (end - start) as u8;
                        let inner_pos = base.inner_pos + ((start as u16) << axis_shift);

                        surfaces.push(VoxelSurface {
                            inner_pos,
                            extent,
                            colour_idx: layers.texture_idx(inner_pos),
                            ..base
                        });
                    }
                }

                for x in 0..32 {
                    let face_l = face_l_mat[31-x];
                    let face_r = face_r_mat[31-x];

                    let packed_without_z = pack_chunk_pos(x, y, 0); // z overwritten

                    add_run_lengths(&mut surfaces, chunk_layers, face_l, 5, VoxelSurface {
                        inner_pos: packed_without_z,
                        face: 0,
                        chunk_idx: chunk_num as _,
                        extent: 0,      // overwritten
                        colour_idx: 0,  // overwritten
                    });

                    add_run_lengths(&mut surfaces, chunk_layers, face_r, 5, VoxelSurface {
                        inner_pos: packed_without_z,
                        face: 1,
                        chunk_idx: chunk_num as _,
                        extent: 0,      // overwritten
                        colour_idx: 0,  // overwritten
                    });
                }

                for z in 0..32 {
                    let row = layer_mask[z];
                    if row == 0 { continue; }

                    let row_d = layer_mask_d[z];
                    let row_u = layer_mask_u[z];
                    let row_b = if z != 0 {
                        layer_mask[z-1]
                    } else {
                        neighbor_masks[4].layer(y)[31]
                    };
                    let row_f = if z != 31 {
                        layer_mask[z+1]
                    } else {
                        neighbor_masks[5].layer(y)[0]
                    };

                    let face_d = row & !row_d;
                    let face_u = row & !row_u;
                    let face_b = row & !row_b;
                    let face_f = row & !row_f;

                    let face_merge = face_u | face_d | face_b | face_f;
                    if face_merge == 0 { continue }

                    let packed_without_x = pack_chunk_pos(0, y, z); // x channel overwritten
                    add_run_lengths(&mut surfaces, chunk_layers, face_d, 0, VoxelSurface {
                        inner_pos: packed_without_x, 
                        face: 2,
                        chunk_idx: chunk_num as _,
                        extent: 0,      // overwritten
                        colour_idx: 0,  // overwritten
                    });

                    add_run_lengths(&mut surfaces, chunk_layers, face_d, 0, VoxelSurface {
                        inner_pos: packed_without_x, 
                        face: 2,
                        chunk_idx: chunk_num as _,
                        extent: 0,
                        colour_idx: 0,
                    });

                    add_run_lengths(&mut surfaces, chunk_layers, face_u, 0, VoxelSurface {
                        inner_pos: packed_without_x, 
                        face: 3,
                        chunk_idx: chunk_num as _,
                        extent: 0,
                        colour_idx: 0,
                    });
                    
                    add_run_lengths(&mut surfaces, chunk_layers, face_b, 0, VoxelSurface {
                        inner_pos: packed_without_x, 
                        face: 4,
                        chunk_idx: chunk_num as _,
                        extent: 0,
                        colour_idx: 0,
                    });

                    add_run_lengths(&mut surfaces, chunk_layers, face_f, 0, VoxelSurface {
                        inner_pos: packed_without_x, 
                        face: 5,
                        chunk_idx: chunk_num as _,
                        extent: 0,
                        colour_idx: 0,
                    });
                }

                layer_mask_d = layer;
            }
        }

        println!("surface calculation: {}us", t.elapsed().as_micros());

        surfaces
    }
}

fn main() {
    let mut camera = Camera {
        pos: Vec3::new(0.0, 0.0, 0.0),
        angle: 0.0,
    };

    const VOXEL_COLOURS: &'static [[f32; 4]] = &[
        [1.0, 0.0, 0.0, 1.0],
        [0.0, 1.0, 0.0, 1.0],
        [0.0, 0.0, 1.0, 1.0],
    ];

    let mut chunks = Chunks::new();
    chunks.load(
        &[IVec3::new(0, 0, 0)],
        &[ChunkToAdd {
            //layers: &[[1u16; 32*32]],
            layers: &[[0, 1, 2, 3, 0, 1, 2, 3, ..0]],
            layer_start: 10,
        }],
    );
    
    let ctx = Ctx::new();

    let camera_uniform = ctx.create_uniform(&camera.to_matrix());
    let loaded_chunk_buffer = ctx.create_storage_buffer(&chunks.refs);
    let surfaces = chunks.calculate_surfaces();
    let surface_buffer = ctx.create_storage_buffer(&surfaces);
    let voxel_colour_buffer = ctx.create_storage_buffer(VOXEL_COLOURS);

    let surface_render = ctx.create_render_pipeline_ex(RenderPipelineDescriptorEx {
        inputs: &[
            PipelineInput::Uniform(&camera_uniform),
            PipelineInput::StorageBuffer(&loaded_chunk_buffer),
            PipelineInput::StorageBuffer(&surface_buffer),
            PipelineInput::StorageBuffer(&voxel_colour_buffer),
        ],
        vertex_buffer: Either::B(wgpu::PrimitiveTopology::TriangleStrip),
        instance_buffer: None,
        shader: ShaderSource::File(std::path::Path::new("examples/voxel/shader.wgsl")),
        shader_vertex_entry: "vertex",
        shader_fragment_entry: "fragment",
        output_format: OUTPUT_TEXTURE_FORMAT,
        blend_state: None,
        cull_mode: Some(wgpu::Face::Back),
        draw_range: 0..4,
        instance_range: 0..surface_buffer.len(),

        disable_depth_test: false,
    });

    let mut timer = ctx.create_timer();
    ctx.run((1024, 1024), 60, |encoder, output, delta, input| {
        let mut cam_changed = false;
        const SPEED: f32 = 12.0;
        if input.held(Key::KeyW) { camera.move_dir(Vec3::Z*SPEED*delta); cam_changed = true; }
        if input.held(Key::KeyA) { camera.move_dir(Vec3::NEG_X*SPEED*delta); cam_changed = true; }
        if input.held(Key::KeyS) { camera.move_dir(Vec3::NEG_Z*SPEED*delta); cam_changed = true; }
        if input.held(Key::KeyD) { camera.move_dir(Vec3::X*SPEED*delta); cam_changed = true; }
        if input.held(Key::KeyQ) { camera.move_dir(Vec3::Y*SPEED*delta); cam_changed = true; }
        if input.held(Key::KeyE) { camera.move_dir(Vec3::NEG_Y*SPEED*delta); cam_changed = true; }

        const ANGULAR_SPEED: f32 = 2.0; // radians per second
        if input.held(Key::ArrowLeft)  { 
            camera.angle = (camera.angle+ANGULAR_SPEED*delta).rem_euclid(2.0*std::f32::consts::PI); 
            cam_changed = true; 
        }
        if input.held(Key::ArrowRight)  { 
            camera.angle = (camera.angle-ANGULAR_SPEED*delta).rem_euclid(2.0*std::f32::consts::PI); 
            cam_changed = true; 
        }

        if cam_changed {
            camera_uniform.update(&ctx, &camera.to_matrix());
        }

        timer.start(encoder);
        ctx.run_render_pass(encoder, output, wgpu::Color::BLACK, &[&surface_render]);
        timer.split(encoder, "render pass");
        timer.print(encoder);

        if input.just_pressed(Key::Escape) { Some(WindowTask::Exit) } else { None }
    });
}
