#![allow(unused)]

use enum_iterator::{all, Sequence};
use std::io::{self, stdout, Read, Stdout, Write};
use std::sync::mpsc::{channel, Receiver};
use std::{
    collections::HashMap,
    fs::File,
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

    fn check_if_taxiway(self) -> bool {
        match self {
            MapPoint::Taxiway(_) => true,
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
        gate: String,
        do_not_go_deep: bool,
    ) -> (bool, Direction) {
        let directions = vec![
            Direction::North,
            Direction::South,
            Direction::East,
            Direction::West,
        ];
        for direction in directions {
            if do_not_go_deep {
                if direction
                    .to_owned()
                    .fetch_mappoint(map, position)
                    .check_if_gate_taxi_line()
                {
                    return (true, direction);
                }
            }
            if self
                .to_owned()
                .check_for_gate_taxi_line(map, position, &gate, direction.to_owned())
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

#[derive(Debug, Clone, Sequence, PartialEq)]
enum AtGateAction {
    ShutdownProcedure,
    DeboardPassengers,
    DeboardCargo,
    UnloadBaggage,
    UnloadCargo,
    Refuel,
    Repair,
    Clean,
    LoadCargo,
    CrewChange,
    MaintenanceCheck,
    LoadBaggage,
    LoadPassengers,
    BoardPassengers,
    LoadAdditionalCargo,
    Standby,
}

#[derive(Debug, Clone)]
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
    AtGate((String, AtGateAction)), // Gate number, wait time
}

#[derive(Debug, Clone)]
struct Plane {
    id: usize,
    name: String,
    current_action: Action,
    position: (usize, usize),
    runway: Runway,
    out_of_map: bool,
}

impl Plane {
    pub fn new(self) -> Self {
        Self {
            id: self.id,
            name: self.name,
            current_action: self.current_action,
            position: self.position,
            runway: self.runway,
            out_of_map: self.out_of_map,
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
        // current_action: Action::TaxiToGate("6".to_string()),
        current_action: Action::Takeoff,
        position: (5, 9),
        runway: runways["1"].clone(),
        out_of_map: false,
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
fn update_game_state(
    airport: &mut Airport,
    time: &Time,
    schedule: &Scheduling,
    score: &Score,
    receiver: &Receiver<String>,
) {
    // Update aircraft position
    update_aircraft_position(airport);
    // Handle user input
    update_aircraft_from_user_input(airport, receiver);
    // Detect collisions
    // Signal alerts
    // Update score
    // Update weather
    // Check and spawn new aircraft
    render(airport);
}

fn render(airport: &Airport) {
    // Draw the airport map to the screen
    let mut stdout = stdout();
    // Move the cursor to the beginning of the terminal
    stdout.write_all(b"\x1B[1;1H").unwrap();
    for (col_index, col) in airport.map.map.iter().enumerate() {
        for (row_index, row) in col.iter().enumerate() {
            // check if plane is at this point
            if airport.planes.iter().any(|plane| {
                plane.position.0 == col_index && plane.position.1 == row_index && !plane.out_of_map
            }) {
                stdout.write_all(b"X").unwrap();
                continue;
            }
            let mut pixel = match row {
                MapPoint::Empty => " ",
                MapPoint::Runway((_, dir)) => match dir {
                    Direction::North | Direction::South => "||",
                    Direction::East | Direction::West => "=",
                    _ => " ",
                },
                MapPoint::Taxiway((_, dir)) => match dir {
                    Direction::North => "^",
                    Direction::South => "v",
                    Direction::East => "<",
                    Direction::West => ">",
                    _ => " ",
                },
                MapPoint::Gate(name) => name,
                MapPoint::GateTaxiLine((_, dir)) => match dir {
                    Direction::North => "^",
                    Direction::South => "v",
                    Direction::East => "<",
                    Direction::West => ">",
                    _ => " ",
                },
            };
            stdout.write_all(pixel.as_bytes()).unwrap();
        }
        stdout.write_all(b"\r\n").unwrap();
    }
    // Flush the output buffer to ensure that the output is immediately displayed
    stdout.flush().unwrap();
}

fn update_aircraft_from_user_input(airport: &mut Airport, receiver: &Receiver<String>) {
    let plane = handle_user_input(receiver, &airport.planes, &airport.runways);
    if plane.is_some() {
        let keep_aside_fleet = airport.planes.clone();
        airport.planes = vec![plane.unwrap()];
        update_aircraft_position(airport);
        // Restore the fleet but replace the plane that was changed
        airport.planes = keep_aside_fleet
            .iter()
            .map(|p| {
                if p.id == airport.planes[0].id {
                    airport.planes[0].to_owned()
                } else {
                    p.to_owned()
                }
            })
            .collect::<Vec<Plane>>();
    }
}

fn update_aircraft_position(airport: &mut Airport) {
    // Update aircraft position
    for plane in airport
        .planes
        .iter_mut()
        .filter(|p| !p.out_of_map)
        .into_iter()
    {
        match &mut plane.current_action {
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
                    .check_for_gate_taxi_line_all_directions(
                        &airport.map,
                        plane.position,
                        gate.to_string(),
                        false,
                    );

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
                            // Change action to AtGate with wait time 0
                            plane.current_action =
                                Action::AtGate((gate.clone(), AtGateAction::ShutdownProcedure));
                            Direction::StayPut
                        }
                        _ => panic!("Plane is not standing on a taxiway or correct gate"),
                    };
                    plane.position = dir.go(plane.position);
                }
            }
            Action::Takeoff => {
                // Check if the plane is out of the map
                if plane.position.0 <= 1
                    || plane.position.0 >= airport.map.map.len() - 1 as usize
                    || plane.position.1 <= 1
                    || plane.position.1 >= airport.map.map[0].len() - 1 as usize
                {
                    plane.out_of_map = true;
                    continue;
                }

                let point = airport.map.map[plane.position.0][plane.position.1].clone();
                match point {
                    MapPoint::Runway((_, _)) | MapPoint::Empty => {
                        plane.position = plane.runway.side.clone().go(plane.position)
                    }
                    _ => panic!("Plane is not standing on a runway"),
                }
            }
            Action::HoldPosition => {}
            Action::TaxiOntoRunway => {
                let point = airport.map.map[plane.position.0][plane.position.1].clone();
                match point {
                    MapPoint::Taxiway((_, dir)) => plane.position = dir.go(plane.position),
                    MapPoint::Runway((name, dir)) => match name {
                        0 => plane.current_action = Action::HoldPosition,
                        _ => plane.position = dir.go(plane.position),
                    },
                    _ => panic!("Plane is not standing on a taxiway or runway"),
                }
            }
            Action::HoldShort => {
                let point = airport.map.map[plane.position.0][plane.position.1].clone();
                match point {
                    MapPoint::Taxiway((_, dir)) => {
                        match dir
                            .to_owned()
                            .fetch_mappoint(&airport.map, plane.position)
                            .check_if_runway()
                        {
                            true => plane.current_action = Action::HoldPosition,
                            false => plane.position = dir.go(plane.position),
                        }
                    }
                    _ => panic!("Plane is not standing on a taxiway"),
                }
            }
            Action::TaxiToRunway(_) => {}
            Action::Pushback => {
                let mut point = airport.map.map[plane.position.0][plane.position.1].clone();
                match point {
                    MapPoint::GateTaxiLine((_, dir)) => {
                        plane.position = dir.get_opposite_dir().go(plane.position);
                        point = airport.map.map[plane.position.0][plane.position.1].clone();
                        if point.check_if_taxiway() {
                            plane.current_action = Action::HoldPosition;
                        }
                    }
                    MapPoint::Gate(ref gate) => {
                        let (is_nearby_gate, mut gate_dir) =
                            point.clone().check_for_gate_taxi_line_all_directions(
                                &airport.map,
                                plane.position,
                                gate.to_string(),
                                true,
                            );
                        match is_nearby_gate {
                            true => plane.position = gate_dir.go(plane.position),
                            false => panic!("Plane is not standing near a gate taxi line"),
                        }
                    }
                    _ => panic!("Plane is not standing at a gate or gate taxi line"),
                };
            }
            Action::AtGate((gate, ref mut atgate_action)) => {
                let actions = all::<AtGateAction>().collect::<Vec<_>>();
                let mut iter = actions.iter();
                while let Some(action) = iter.next() {
                    if action.to_owned() == atgate_action.to_owned() {
                        match iter.next() {
                            Some(next_action) => *atgate_action = next_action.to_owned(),
                            None => *atgate_action = AtGateAction::Standby,
                        }
                    }
                }
            }
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

fn handle_user_input(
    receiver: &Receiver<String>,
    planes: &Vec<Plane>,
    runways: &HashMap<String, Runway>,
) -> Option<Plane> {
    // Handle user input and issue appropriate commands to aircraft
    if let Ok(user_input) = receiver.try_recv() {
        let plane = parse_user_input(user_input, planes, runways);
        match plane {
            Ok(plane) => return Some(plane),
            Err(e) => {
                println!("\nError: {:?}\n", e);
                return None;
            }
        }
    }
    None
}

fn parse_user_input(
    command: String,
    planes: &Vec<Plane>,
    runways: &HashMap<String, Runway>,
) -> Result<Plane, String> {
    /*
        Language is:
        l <aircraft> <runway_number>        : Landing at runway X
        t <aircraft> <runway_number>        : Takeoff from runway X
        hp <aircraft>                       : Hold position
        p <aircraft>                        : Pushback
        tor <aircraft> <runway_number>      : Taxi onto runway X
        hs <aircraft> <runway_number>       : Hold short of runway X
        t2r <aircraft> <runway_number>      : Taxi to runway X
        t2g <aircraft> <gate_number>        : Taxi to gate X

        TODO:
        t2t <aircraft> <terminal_number>    : Taxi to terminal X
    */
    let command = command.split_whitespace().collect::<Vec<_>>();
    if command.len() > 3 || command.len() < 2 {
        return Err("Wrong user input length.".to_string());
    }
    let keyword = command[0];
    let aircraft = command[1].to_string();
    let mut plane = planes
        .iter()
        .find(|plane| plane.name == aircraft)
        .ok_or("Plane not found")?
        .clone();

    let valid_commands = ["hp", "p", "l", "t", "tor", "hs", "t2r", "t2g"];
    if !valid_commands.contains(&keyword) {
        return Err("Invalid command: ".to_string() + keyword);
    }
    if keyword != "hp" && keyword != "p" && command.len() != 3 {
        return Err("Must contain a runway/gate/terminal number".to_string());
    }
    let mut destination_num = None;
    if keyword != "hp" && keyword != "p" {
        destination_num = Some(command[2].to_string());
        if keyword != "t2g" {
            // Check if runway exists, and if it does, set the plane's runway
            if !runways.contains_key(&destination_num.clone().unwrap()) {
                return Err("Runway not found".to_string());
            }
            let runway = runways.get(&destination_num.clone().unwrap()).unwrap();
            plane.runway = runway.clone();
        }
    }

    let action = match keyword {
        "l" => Action::Land,
        "t" => Action::Takeoff,
        "hp" => Action::HoldPosition,
        "p" => Action::Pushback,
        "tor" => Action::TaxiOntoRunway,
        "hs" => Action::HoldShort,
        "t2r" => Action::TaxiToRunway(destination_num.unwrap().parse::<usize>().unwrap()),
        "t2g" => Action::TaxiToGate(destination_num.unwrap()),
        _ => Action::HoldPosition, // Should never happen
    };

    /*
        Valid successors for each action:
        InAir: -
        Land: -
        HoldPosition: TaxiToGate (after landing), TaxiToRunway, HoldShort, TaxiOntoRunway
        Pushback: -
        TaxiOntoRunway: HoldPosition, HoldShort, Takeoff, TaxiToRunway
        HoldShort: HoldPosition, TaxiOntoRunway, Takeoff, TaxiToRunway
        TaxiToRunway: HoldPosition, HoldShort, Takeoff, TaxiOntoRunway
        TaxiToGate: HoldPosition
        Takeoff: -
        AtGate: Pushback
    */
    match plane.current_action {
        Action::InAir => return Err("Not a valid action when plane is in the air".to_string()),
        Action::Land => return Err("Not a valid action when in the process of landing".to_string()),
        Action::Takeoff => {
            return Err("Not a valid action when in the process of takeoff".to_string())
        }
        Action::HoldPosition => match action {
            Action::TaxiToGate(_)
            | Action::TaxiToRunway(_)
            | Action::HoldShort
            | Action::TaxiOntoRunway => {}
            _ => {
                return Err("Not a valid action when holding position".to_string());
            }
        },
        Action::TaxiOntoRunway => match action {
            Action::HoldPosition
            | Action::HoldShort
            | Action::Takeoff
            | Action::TaxiToRunway(_) => {}
            _ => {
                return Err("Not a valid action when taxiing onto runway".to_string());
            }
        },
        Action::HoldShort => match action {
            Action::HoldPosition
            | Action::TaxiOntoRunway
            | Action::Takeoff
            | Action::TaxiToRunway(_) => {}
            _ => {
                return Err("Not a valid action when holding short".to_string());
            }
        },
        Action::TaxiToRunway(_) => match action {
            Action::HoldPosition | Action::HoldShort | Action::Takeoff | Action::TaxiOntoRunway => {
            }
            _ => {
                return Err("Not a valid action when taxiing to runway".to_string());
            }
        },
        Action::TaxiToGate(_) => match action {
            Action::HoldPosition => {}
            _ => {
                return Err("Not a valid action when taxiing to gate".to_string());
            }
        },
        Action::Pushback => {
            return Err("Not a valid action when in the process of pushback".to_string())
        }
        Action::AtGate(_) => match action {
            Action::Pushback => {}
            _ => {
                return Err("Not a valid action when at gate".to_string());
            }
        },
    }

    plane.current_action = action;

    Ok(plane)
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

fn user_input_thread(sender: std::sync::mpsc::Sender<String>) {
    loop {
        let mut user_input = String::new();
        io::stdin()
            .read_line(&mut user_input)
            .expect("Failed to read user input");

        // Trim whitespace and newline characters from the input
        user_input = user_input.trim().to_string();

        // Send the user input to the main game loop through the channel
        sender.send(user_input).expect("Failed to send user input");
    }
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

    // Channel for communication between threads
    let (sender, receiver): (std::sync::mpsc::Sender<String>, Receiver<String>) = channel();

    // Separate thread for handling user input
    std::thread::spawn(move || {
        user_input_thread(sender);
    });

    loop {
        // println!(
        //     "\nAirport is: {:?}, At: {:?}\n",
        //     airport.planes[0],
        //     airport.map.map[airport.planes[0].position.0][airport.planes[0].position.1]
        // );
        update_game_state(&mut airport, &time, &scheduling, &score, &receiver);
        render(&mut airport);
        // Sleep for a bit
        thread::sleep(Duration::from_secs(time.step_duration as u64));
    }
}
