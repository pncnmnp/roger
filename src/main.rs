#![allow(unused)]

use std::{
    collections::HashMap,
    fs::File,
    hash::Hash,
    io::{BufRead, BufReader},
    thread,
    time::Duration,
};

#[derive(Clone, PartialEq, Debug)]
enum Direction {
    North,
    South,
    East,
    West,
    StayPut,
}

impl Direction {
    pub fn go(self, position: (usize, usize)) -> (usize, usize) {
        match self {
            Direction::North => (position.0 - 1, position.1),
            Direction::South => (position.0 + 1, position.1),
            Direction::East => (position.0, position.1 + 1),
            Direction::West => (position.0, position.1 - 1),
            Direction::StayPut => (position.0, position.1),
        }
    }

    pub fn fetch_mappoint(self, map: &Map, position: (usize, usize)) -> MapPoint {
        let (x, y) = self.go(position);
        map.map[x][y].clone()
    }

    pub fn get_opposite_dir(self) -> Self {
        match self {
            Direction::North => Direction::South,
            Direction::South => Direction::North,
            Direction::East => Direction::West,
            Direction::West => Direction::East,
            Direction::StayPut => Direction::StayPut,
        }
    }

    pub fn parse(dir: &char) -> Result<Self, String> {
        match dir {
            'N' => Ok(Direction::North),
            'S' => Ok(Direction::South),
            'E' => Ok(Direction::East),
            'W' => Ok(Direction::West),
            'X' => Ok(Direction::StayPut),
            _ => Err(format!("Invalid direction: {}", dir)),
        }
    }
}

#[derive(Debug, Clone)]
struct Runway {
    name: usize,
    side: Direction,
}

impl Runway {
    pub fn new(map: &Map) -> HashMap<String, Self> {
        let mut runways: HashMap<String, Self> = HashMap::new();
        for (i, row) in map.map.iter().enumerate() {
            for (j, col) in row.iter().enumerate() {
                if let MapPoint::Runway((name, side)) = col {
                    let mut is_unique = true;
                    for runway in &runways {
                        if runways.contains_key(&name.to_string()) {
                            is_unique = false;
                        }
                    }
                    if is_unique {
                        runways.insert(
                            name.to_string(),
                            Runway {
                                name: name.clone(),
                                side: side.clone(),
                            },
                        );
                    }
                }
            }
        }
        runways
    }
}

#[derive(Clone, Debug)]
struct Gate {
    number: String,
    is_occupied: bool,
}

impl Gate {
    pub fn new(map: &Map) -> HashMap<String, Self> {
        let mut gates: HashMap<String, Self> = HashMap::new();
        for (i, row) in map.map.iter().enumerate() {
            for (j, col) in row.iter().enumerate() {
                if let MapPoint::Gate(number) = col {
                    let mut is_unique = true;
                    for gate in &gates {
                        if gates.contains_key(&number.to_string()) {
                            panic!("Duplicate gate number: {}", number);
                        }
                    }
                    if is_unique {
                        gates.insert(
                            number.to_string(),
                            Gate {
                                number: number.clone(),
                                is_occupied: false,
                            },
                        );
                    }
                }
            }
        }
        gates
    }
}

#[derive(Clone, PartialEq, Debug)]
enum MapPoint {
    Runway((usize, Direction)),
    Taxiway((usize, Direction)),
    Gate(String),
    Plane(String),
    GateTaxiLine((String, Direction)),
    Empty,
}

impl MapPoint {
    fn check_if_runway(self) -> bool {
        match self {
            MapPoint::Runway(_) => true,
            _ => false,
        }
    }

    fn check_if_gate_taxi_line(self) -> bool {
        match self {
            MapPoint::GateTaxiLine(_) => true,
            _ => false,
        }
    }

    fn check_for_gate_taxi_line(
        self,
        map: &Map,
        position: (usize, usize),
        gate: &str,
        direction: Direction,
    ) -> bool {
        // Search all directions for a gate taxi line
        if direction
            .to_owned()
            .fetch_mappoint(map, position)
            .check_if_gate_taxi_line()
        {
            let new_pos = direction.to_owned().go(position);
            return self.check_for_gate_taxi_line(map, new_pos, gate, direction);
        } else if direction.fetch_mappoint(map, position).check_if_gate(gate) {
            return true;
        }
        false
    }

    fn check_for_gate_taxi_line_all_directions(
        self,
        map: &Map,
        position: (usize, usize),
        gate: &str,
    ) -> (bool, Direction) {
        let directions = vec![
            Direction::North,
            Direction::South,
            Direction::East,
            Direction::West,
        ];
        for direction in directions {
            if self
                .to_owned()
                .check_for_gate_taxi_line(map, position, gate, direction.to_owned())
            {
                return (true, direction);
            }
        }
        (false, Direction::StayPut)
    }

    fn check_if_gate(self, gate: &str) -> bool {
        match self {
            MapPoint::Gate(number) => number == gate,
            _ => false,
        }
    }
}

#[derive(Debug)]
struct Map {
    length: usize,
    width: usize,
    spacing: usize,
    map: Vec<Vec<MapPoint>>,
}

#[derive(Debug)]
enum WeatherCondition {
    Clear,
    Rain,
    InclementWeather,
}

#[derive(Debug)]
enum Action {
    InAir,
    Land,
    Takeoff,
    HoldPosition,
    TaxiOntoRunway,
    HoldShort,
    TaxiToRunway(usize),
    TaxiToGate(String),
    Pushback,
    AtGate(String),
}

#[derive(Debug)]
struct Plane {
    id: usize,
    name: String,
    current_action: Action,
    position: (usize, usize),
    runway: Runway,
}

impl Plane {
    pub fn new(self) -> Self {
        Self {
            id: self.id,
            name: self.name,
            current_action: self.current_action,
            position: self.position,
            runway: self.runway,
        }
    }
}

#[derive(Debug)]
struct Airport {
    runways: HashMap<String, Runway>,
    gates: HashMap<String, Gate>,
    map: Map,
    weather: WeatherCondition,
    planes: Vec<Plane>,
}

struct Time {
    step_duration: usize, // Duration in seconds for each game step
}

struct GroundAlert {
    message: String,
}

struct Scheduling {
    landing_interval: usize,            // Interval for planes landing in steps
    background_actions_duration: usize, // Duration in steps for background actions
}

struct Score {
    land: usize,
    takeoff: usize,
    crash: usize,
}

impl Score {
    pub fn score(self) -> i32 {
        (self.land + self.takeoff - (100 * self.crash)) as i32
    }
}

fn construct_airport() -> Airport {
    let spacing = 5;
    let map_path = "./src/airport.map";
    let map = build_airport_map(map_path, spacing);

    let runways = Runway::new(&map);
    let gates = Gate::new(&map);
    let weather = WeatherCondition::Clear;

    let planes = vec![Plane {
        id: 0,
        name: "AA117".to_string(),
        // current_action: Action::TaxiToGate("1".to_string()),
        current_action: Action::InAir,
        position: (5, 0),
        runway: runways["1"].clone(),
    }];

    Airport {
        runways,
        gates,
        map,
        weather,
        planes,
    }
}

fn build_airport_map(map_path: &str, spacing: usize) -> Map {
    // open the map file
    let map_file = File::open(map_path).expect("Failed to open map file");

    // Get the map dimensions present in the first line of the format "XxY"
    let mut map_dimensions = String::new();
    let mut map_file = BufReader::new(map_file);
    map_file
        .read_line(&mut map_dimensions)
        .expect("Failed to read map dimensions");
    let width = map_dimensions
        .split('x')
        .next()
        .expect("Failed to parse map width")
        .parse::<usize>()
        .expect("Failed to parse map width");
    let length = map_dimensions
        .split('x')
        .nth(1)
        .expect("Failed to parse map length")
        .replace("\n", "")
        .parse::<usize>()
        .expect("Failed to parse map length");

    let mut map: Vec<Vec<MapPoint>> = vec![vec![MapPoint::Empty; width]; length];

    // Read the map file line by line and populate the map
    for (y, line) in map_file.lines().enumerate() {
        let line = line.expect("Failed to read line in map");
        for (x, block) in line.split(",").enumerate() {
            if block == "..." {
                continue;
            }
            let point = block.chars().nth(0).expect("Failed to parse MapPoint");
            let name = block.chars().nth(1).expect("Failed to parse Name");
            let dir_info = block.chars().nth(2).expect("Failed to parse Direction");
            let direction = Direction::parse(&dir_info).expect("Failed to parse Direction");

            let map_point = match point {
                'R' => {
                    let name = name.to_digit(10).expect("Failed to parse Runway Name");
                    MapPoint::Runway((name as usize, direction))
                }
                'T' => {
                    let name = name.to_digit(10).expect("Failed to parse Taxiway Name");
                    MapPoint::Taxiway((name as usize, direction))
                }
                'M' => MapPoint::GateTaxiLine((name.to_string(), direction)),
                'G' => MapPoint::Gate(name.to_string()),
                _ => MapPoint::Empty,
            };
            map[y][x] = map_point;
        }
    }

    // Add spacing of MapPoint::Empty on all sides of map rows
    let mut map = map
        .iter()
        .map(|row| {
            let mut row = row.clone();
            for _ in 0..spacing {
                row.insert(0, MapPoint::Empty);
                row.push(MapPoint::Empty);
            }
            row
        })
        .collect::<Vec<Vec<MapPoint>>>();
    // Add spacing num of columns on top and bottom
    for _ in 0..spacing {
        let mut row = vec![MapPoint::Empty; width + (spacing * 2)];
        map.insert(0, row.clone());
        map.push(row);
    }

    Map {
        length,
        width,
        spacing,
        map,
    }
}

// Function to update the game state for each time step
fn update_game_state(airport: &mut Airport, time: &Time, schedule: &Scheduling, score: &Score) {
    // Update aircraft position
    update_aircraft_position(airport);
    // Detect collisions
    // Signal alerts
    // Handle user input
    // Update score
    // Update weather
    // Check and spawn new aircraft
}

fn render(airport: &Airport) {}

fn update_aircraft_position(airport: &mut Airport) {
    // Update aircraft position
    for plane in airport.planes.iter_mut() {
        match &plane.current_action {
            Action::InAir => {
                let mut plane_dir = Direction::StayPut;
                let pos = match plane.runway.side {
                    Direction::West | Direction::East | Direction::North | Direction::South => {
                        plane_dir = plane.runway.side.clone();
                        plane_dir.to_owned().go(plane.position)
                    }
                    Direction::StayPut => todo!(),
                };
                plane.position = pos;

                // Check if plane has reached the start of the runway
                let runway_name = plane.runway.name;
                if Direction::StayPut.fetch_mappoint(&airport.map, plane.position)
                    == MapPoint::Runway((runway_name, plane_dir))
                {
                    plane.current_action = Action::Land;
                }
            }
            Action::Land => {
                let pos = match plane.runway.side {
                    Direction::West | Direction::East | Direction::North | Direction::South => {
                        let plane_dir = plane.runway.side.clone();
                        let pos = plane_dir.to_owned().go(plane.position);
                        // Check if plane has reached the end of the runway
                        if plane_dir.fetch_mappoint(&airport.map, pos) == MapPoint::Empty {
                            plane.current_action = Action::HoldPosition;
                        }
                        pos
                    }
                    Direction::StayPut => todo!(),
                };
                plane.position = pos;
            }
            Action::TaxiToGate(gate) => {
                // Check if the plane is standing at the end of the runway
                if airport.map.map[plane.position.0][plane.position.1]
                    .clone()
                    .check_if_runway()
                    && plane
                        .runway
                        .side
                        .clone()
                        .fetch_mappoint(&airport.map, plane.position)
                        == MapPoint::Empty
                {
                    // Change position from runway to taxiway
                    let point = airport.map.map[plane.position.0][plane.position.1].clone();
                    let taxiway_dir = match point {
                        MapPoint::Runway((_, dir)) => dir,
                        _ => panic!("Plane is not standing on a runway"),
                    };
                    plane.position = taxiway_dir.go(plane.position);
                    continue;
                }
                // Check if there is a GateTaxiLine in any direction surrounding the current direction
                let (is_nearby_gate, gate_dir) = airport.map.map[plane.position.0]
                    [plane.position.1]
                    .clone()
                    .check_for_gate_taxi_line_all_directions(&airport.map, plane.position, gate);

                if is_nearby_gate {
                    plane.position = gate_dir.go(plane.position);
                }
                // Traverse along the taxiway/gate line
                else {
                    let point = airport.map.map[plane.position.0][plane.position.1].clone();
                    let dir = match point {
                        MapPoint::Taxiway((_, dir)) => dir,
                        MapPoint::GateTaxiLine((_, dir)) => dir,
                        MapPoint::Gate(_) => {
                            // Gate is now occupied
                            let at = airport.gates.get_mut(gate).expect("Gate not found");
                            at.is_occupied = true;
                            // Change action to AtGate
                            plane.current_action = Action::AtGate(gate.clone());
                            Direction::StayPut
                        }
                        _ => panic!("Plane is not standing on a taxiway or correct gate"),
                    };
                    plane.position = dir.go(plane.position);
                }
            }
            Action::Takeoff => {}
            Action::HoldPosition => {}
            Action::TaxiOntoRunway => {}
            Action::HoldShort => {}
            Action::TaxiToRunway(_) => {}
            Action::Pushback => {}
            Action::AtGate(_) => {}
        }
    }
}

// Function to detect and handle collisions
fn detect_and_handle_collisions(airport: &mut Airport) {
    // Detect and resolve collisions between aircraft
}

// Function to handle ground staff alerts
fn handle_ground_alerts(airport: &mut Airport, alert: GroundAlert) {
    // Take appropriate actions in response to ground staff alerts
}

// Function to handle user input and issue commands
fn handle_user_input(airport: &mut Airport, action: Action) {
    // Handle user input and issue appropriate commands to aircraft
}

fn update_score(airport: &mut Airport, score: &Score) {
    // Update the score based on the current game state
}

// Function to simulate weather conditions
fn simulate_weather(airport: &mut Airport, condition: WeatherCondition) {
    // Simulate the impact of weather conditions on aircraft operations
}

fn spawn_landing_aircraft(airport: &mut Airport, schedule: &Scheduling) {
    // Spawn new aircraft for landing
}

// Main function to run the game
fn main() {
    // Initialize and run your ATC game here
    let mut airport = construct_airport();
    let time = Time { step_duration: 1 };
    let scheduling = Scheduling {
        landing_interval: 12,
        background_actions_duration: 12,
    };
    let score = Score {
        land: 0,
        takeoff: 0,
        crash: 0,
    };

    loop {
        println!(
            "\nAirport is: {:?}, At: {:?}\n",
            airport.planes[0],
            airport.map.map[airport.planes[0].position.0][airport.planes[0].position.1]
        );
        update_game_state(&mut airport, &time, &scheduling, &score);
        render(&mut airport);
        // Sleep for a bit
        thread::sleep(Duration::from_secs(time.step_duration as u64));
    }
}
