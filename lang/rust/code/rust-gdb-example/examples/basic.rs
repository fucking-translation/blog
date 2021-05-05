use rust_gdb_example::{Animal, AnimalType};

fn main() {

    let animals = vec![
        Animal {
            kind: AnimalType::Cat,
            name: "Chip".to_string(),
            age: 4,
        },
        Animal {
            kind: AnimalType::Cat,
            name: "Nacho".to_string(),
            age: 6,
        },
        Animal {
            kind: AnimalType::Dog,
            name: "Taco".to_string(),
            age: 2
        }
    ];
    get_chip(&animals);
}

fn get_chip(animals: &Vec<Animal>) {
    let chip = animals.get(0);
    println!("chip: {:?}", chip);
}