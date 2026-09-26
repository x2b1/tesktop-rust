use opus2::{Application, MSEncoder};

fn main() {
	// The new struct-based API is much more readable!
	let surround = MSEncoder::new_surround(48000, 6, 1, Application::Audio)
		.expect("Failed to create surround encoder");

	println!("=== Surround Encoder Configuration ===");
	println!("Streams: {}", surround.streams);
	println!("Coupled streams: {}", surround.coupled_streams);
	println!("Channel mapping: {:?}", surround.mapping);

	// You can destructure if you want specific fields
	let opus2::SurroundEncoder {
		encoder: _encoder,
		streams,
		coupled_streams,
		mapping,
	} = surround;

	println!("\n=== Destructured Values ===");
	println!("Streams: {}", streams);
	println!("Coupled streams: {}", coupled_streams);
	println!("Mapping: {:?}", mapping);

	// Now you can use the encoder for encoding
	// encoder.encode(...);

	println!("\nEncoder is ready for use!");
}
