fn main() {
    println!("Hello, world!");

    for dev in block_utils::get_block_devices().unwrap() {
        println!("Found block device: {}", dev.display());
    }
    
}
