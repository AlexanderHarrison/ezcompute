fn main() {
    let size = (16, 16);
    let mut rgb_buf = vec![0u8; size.0 * size.1 * 4];
    let mut y_buf = vec![0u8; size.0 * size.1];
    let mut u_buf = vec![0u8; size.0 * size.1];
    let mut v_buf = vec![0u8; size.0 * size.1];

    convert_rgba_to_yuv420p(&rgb_buf, size.0, size.1, &mut y_buf, &mut u_buf, &mut v_buf);

    let frame_rate = 60.0;
    let frame_count = 60;
    let file = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .open(std::path::Path::new("video.mkv")).expect("could not open or create file");
    let mut encoder = y4m::encode(size.0, size.1, y4m::Ratio::new(60, 1))
        .with_colorspace(y4m::Colorspace::C444)
        .write_header(file)
        .unwrap();

    let mut frame_num = 0;
    loop {
        let frame = y4m::Frame::new([&y_buf, &u_buf, &v_buf], None);
        if frame_num == frame_count { break }
        frame_num += 1;
        print!("encoding frame {}/{}\r", frame_num, frame_count);
        encoder.write_frame(&frame).unwrap();
    }
}

