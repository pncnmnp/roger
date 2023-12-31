#[cfg(target_os = "macos")]
use cocoa_foundation::base::id;
use cocoa_foundation::foundation::NSDefaultRunLoopMode;
#[cfg(target_os = "macos")]
use cocoa_foundation::foundation::NSRunLoop;
use objc::class;
#[cfg(target_os = "macos")]
use objc::{msg_send, sel, sel_impl};

use clap::{ArgAction, Parser};
use enum_iterator::{all, Sequence};
use lazy_static::lazy_static;
use rand::seq::SliceRandom;
use rand::Rng;
use rand_distr::{Distribution, Normal};
use std::io::{self, stdout, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{channel, Receiver};
use std::sync::Mutex;
use std::{
    collections::HashMap,
    fs::File,
    io::{BufRead, BufReader},
    thread,
    time::Duration,
};
use tts::*;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// ATC command mode or simulation mode
    #[arg(short, long, action = ArgAction::SetTrue)]
    sim: bool,
}

// Stores the latest error message
struct Message {
    message: String,
    timer: AtomicUsize,
}
lazy_static! {
    static ref ERROR: Mutex<Message> = Mutex::new(Message {
        message: String::new(),
        timer: AtomicUsize::new(0),
    });
}
lazy_static! {
    static ref ATC: Mutex<Message> = Mutex::new(Message {
        message: String::new(),
        timer: AtomicUsize::new(0),
    });
}
// Message from Airport Operations Center
lazy_static! {
    static ref AOC: Mutex<Message> = Mutex::new(Message {
        message: String::new(),
        timer: AtomicUsize::new(0),
    });
}

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
        for row in map.map.iter() {
            for col in row.iter() {
                if let MapPoint::Runway((name, side)) = col {
                    let mut is_unique = true;
                    if runways.contains_key(&name.to_string()) {
                        is_unique = false;
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
    position: (usize, usize),
    is_occupied: bool,
}

impl Gate {
    pub fn new(map: &Map) -> HashMap<String, Self> {
        let mut gates: HashMap<String, Self> = HashMap::new();
        for (row_num, row) in map.map.iter().enumerate() {
            for (col_num, col) in row.iter().enumerate() {
                if let MapPoint::Gate(number) = col {
                    if gates.contains_key(&number.to_string()) {
                        panic!("Duplicate gate number: {}", number);
                    }
                    gates.insert(
                        number.to_string(),
                        Gate {
                            number: number.clone(),
                            position: (row_num, col_num),
                            is_occupied: false,
                        },
                    );
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

    fn check_for_taxiway(self, map: &Map, position: (usize, usize)) -> (bool, Direction) {
        // Search all directions for a taxiway
        for direction in vec![
            Direction::North,
            Direction::South,
            Direction::East,
            Direction::West,
        ] {
            if direction
                .to_owned()
                .fetch_mappoint(map, position)
                .check_if_taxiway()
            {
                return (true, direction);
            }
        }
        (false, Direction::StayPut)
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

#[derive(Debug, Clone)]
struct Spacing {
    top_bottom: usize,
    left_right: usize,
}

#[derive(Debug)]
struct Map {
    _length: usize,
    _width: usize,
    spacing: Spacing,
    map: Vec<Vec<MapPoint>>,
}

#[derive(Debug, PartialEq)]
enum WeatherCondition {
    Clear,
    Rain,
    InclementWeather,
}

#[derive(Debug)]
struct Weather {
    condition: WeatherCondition,
    wind_direction: usize, // 0-360 degrees
    wind_speed: f64,       // 0-60 knots
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
    TaxiOntoRunway(usize),
    HoldShort,
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

lazy_static! {
    static ref AIRWAY_IDS: HashMap<&'static str, &'static str> = {
        let mut map = HashMap::new();
        map.insert("AA", "American Airlines");
        map.insert("DL", "Delta Air Lines");
        map.insert("UA", "United Airlines");
        map.insert("BA", "British Airways");
        map.insert("AF", "Air France");
        map.insert("LH", "Lufthansa");
        map.insert("EK", "Emirates");
        map.insert("QF", "Qantas");
        map.insert("AS", "Alaska Airlines");
        map.insert("WN", "Southwest Airlines");
        map.insert("AI", "Air India");
        map
    };
}

#[derive(Debug)]
struct Airport {
    runways: HashMap<String, Runway>,
    gates: HashMap<String, Gate>,
    map: Map,
    weather: Weather,
    planes: Vec<Plane>,
}

struct Time {
    step_duration: usize, // Duration in seconds for each game step
}

struct _GroundAlert {
    message: String,
}

struct Score {
    takeoff: usize,
    crash: usize,
}

impl Score {
    pub fn _score(self) -> i32 {
        (self.takeoff - (100 * self.crash)) as i32
    }
}

fn construct_airport() -> Airport {
    let spacing = Spacing {
        top_bottom: 2,
        left_right: 20,
    };
    let map_path = "./src/airport.map";
    let map = build_airport_map(map_path, spacing.clone());

    let runways = Runway::new(&map);
    let gates = Gate::new(&map);
    let mut weather = Weather {
        condition: WeatherCondition::Clear,
        wind_direction: 360,
        wind_speed: 0.0,
    };
    simulate_wind_direction_and_speed(&mut weather, 100);

    Airport {
        runways,
        gates,
        map,
        weather,
        planes: vec![],
    }
}

fn build_airport_map(map_path: &str, spacing: Spacing) -> Map {
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

    // Add spacing of MapPoint::Empty on left/right sides of map rows
    let mut map = map
        .iter()
        .map(|row| {
            let mut row = row.clone();
            for _ in 0..spacing.left_right {
                row.insert(0, MapPoint::Empty);
                row.push(MapPoint::Empty);
            }
            row
        })
        .collect::<Vec<Vec<MapPoint>>>();
    // Add spacing num of columns on top and bottom
    for _ in 0..spacing.top_bottom {
        let row = vec![MapPoint::Empty; width + (spacing.left_right * 2)];
        map.insert(0, row.clone());
        map.push(row);
    }

    Map {
        _length: length,
        _width: width,
        spacing,
        map,
    }
}

// Function to update the game state for each time step
fn update_game_state(
    airport: &mut Airport,
    spawn_plane: bool,
    score: &mut Score,
    receiver: &Receiver<String>,
    tts: &mut Tts,
) {
    update_aircraft_position(airport);
    update_aircraft_from_user_input(airport, receiver, tts);
    // Signal alerts
    update_score(airport, score);
    simulate_weather(airport);
    if spawn_plane {
        spawn_landing_aircraft(airport, false);
    }
    render(airport, score);
    detect_and_handle_collisions(airport, score);
}

fn render(airport: &Airport, score: &Score) {
    // Draw the airport map to the screen
    let mut stdout = stdout();
    // Clear the screen
    stdout.write_all(b"\x1B[2J").unwrap();
    // Move the cursor to the beginning of the terminal
    stdout.write_all(b"\x1B[1;1H").unwrap();

    // Print the dashboard
    let weather = format!("{:?}", airport.weather.condition);
    stdout
        .write_all(
            format!(
                "Takeoffs: {:<5} Weather: {:<20} Wind Direction: {}'   Wind Speed: {:.2} kn\n",
                score.takeoff, weather, airport.weather.wind_direction, airport.weather.wind_speed
            )
            .as_bytes(),
        )
        .unwrap();

    for (col_index, col) in airport.map.map.iter().enumerate() {
        for (row_index, row) in col.iter().enumerate() {
            // check if plane is at this point
            let mut plane_rendered = false;
            for plane in airport.planes.iter() {
                if plane.position.0 == col_index
                    && plane.position.1 == row_index
                    && !plane.out_of_map
                {
                    let dir: Direction = match row {
                        MapPoint::GateTaxiLine((_, dir))
                        | MapPoint::Runway((_, dir))
                        | MapPoint::Taxiway((_, dir)) => dir.clone(),
                        MapPoint::Gate(gate) => {
                            let point = row.clone();
                            point
                                .check_for_gate_taxi_line_all_directions(
                                    &airport.map,
                                    (col_index, row_index),
                                    gate.to_string(),
                                    true,
                                )
                                .1
                                .get_opposite_dir()
                        }
                        MapPoint::Empty => plane.runway.side.clone(),
                    };
                    match dir {
                        Direction::North => stdout.write_all("▲".as_bytes()).unwrap(),
                        Direction::South => stdout.write_all("▼".as_bytes()).unwrap(),
                        Direction::East => stdout.write_all("▶".as_bytes()).unwrap(),
                        Direction::West => stdout.write_all("◀".as_bytes()).unwrap(),
                        _ => (),
                    }
                    plane_rendered = true;
                }
            }
            if plane_rendered {
                continue;
            }
            let pixel = match row {
                MapPoint::Empty => " ",
                MapPoint::Runway((usize, dir)) => match usize {
                    0 => "∥",
                    _ => match dir {
                        Direction::North | Direction::South => "∥",
                        Direction::East | Direction::West => "=",
                        _ => " ",
                    },
                },
                MapPoint::Taxiway((_, dir)) => match dir {
                    Direction::North => "^",
                    Direction::South => "v",
                    Direction::East => ">",
                    Direction::West => "<",
                    _ => " ",
                },
                MapPoint::Gate(name) => name,
                MapPoint::GateTaxiLine((_, dir)) => match dir {
                    Direction::North => "↑",
                    Direction::South => "↓",
                    Direction::East => "→",
                    Direction::West => "←",
                    _ => " ",
                },
            };
            stdout.write_all(pixel.as_bytes()).unwrap();
        }
        stdout.write_all(b"\r\n").unwrap();
    }
    // Print out the plane information in a table format on the terminal
    stdout.write_all(b"Planes\r\n").unwrap();
    let header = format!(
        "{}\t{}\t{}\t{:<30}{}\n",
        "ID", "Name", "Runway", "Airlines", "Status"
    );
    stdout.write_all(header.as_bytes()).unwrap();
    for plane in airport.planes.iter().filter(|p| !p.out_of_map) {
        let airline = AIRWAY_IDS.get(plane.name.get(..2).unwrap()).unwrap();
        let info = format!(
            "{}\t{}\t{}\t{:<30}{:?}\n",
            plane.id, plane.name, plane.runway.name, airline, plane.current_action
        );
        stdout.write_all(info.as_bytes()).unwrap();
    }
    stdout.write_all(b"\r\n\n").unwrap();

    // Print out the latest error message
    if let Ok(error) = ERROR.lock() {
        if error.timer.load(Ordering::SeqCst) > 0 {
            stdout
                .write_all(format!("‼  {}", error.message).as_bytes())
                .unwrap();
            error.timer.fetch_sub(1, Ordering::SeqCst);
            stdout.write_all(b"\r\n").unwrap();
        }
    }

    // Print out the latest clearance message
    if let Ok(clearance) = ATC.lock() {
        if clearance.timer.load(Ordering::SeqCst) > 0 {
            stdout
                .write_all(format!("🎙  {}", clearance.message).as_bytes())
                .unwrap();
            clearance.timer.fetch_sub(1, Ordering::SeqCst);
            stdout.write_all(b"\r\n").unwrap();
        }
    }

    // Print out the message from Airport Operations Center (AOC) if any
    if let Ok(aoc) = AOC.lock() {
        if aoc.message.len() > 0 {
            stdout
                .write_all(format!("\n{}", aoc.message).as_bytes())
                .unwrap();
            stdout.write_all(b"\r\n").unwrap();
        }
    }

    // Flush the output buffer to ensure that the output is immediately displayed
    stdout.flush().unwrap();
}

fn update_aircraft_from_user_input(
    airport: &mut Airport,
    receiver: &Receiver<String>,
    tts: &mut Tts,
) {
    if let Ok(user_input) = receiver.try_recv() {
        let plane = parse_user_input(
            user_input,
            &airport.planes,
            &airport.runways,
            &airport.weather,
        );
        if plane.is_ok() {
            let keep_aside_fleet = airport.planes.clone();
            let plane = plane.unwrap();
            airport.planes = vec![plane.clone()];
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

            // Get the clearance message
            let clearance = create_atc_clearance(&airport, &plane);
            tts.speak(clearance.clone(), false)
                .expect("Could not speak ATC clearance");
            #[cfg(target_os = "macos")]
            {
                let run_loop: id = unsafe { NSRunLoop::currentRunLoop() };
                unsafe {
                    let date: id = msg_send![class!(NSDate), distantFuture];
                    let _: () = msg_send![run_loop, runMode:NSDefaultRunLoopMode beforeDate:date];
                }
            }
            if let Ok(mut atc) = ATC.lock() {
                atc.message = clearance;
                atc.timer = AtomicUsize::new(5);
            }
        } else if plane.is_err() {
            if let Ok(mut error) = ERROR.lock() {
                error.message = plane.err().unwrap();
                error.timer = AtomicUsize::new(5);
            }
        }
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
                let plane_dir;
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
                        // Check if plane has a nearby taxiway
                        let (nearby_taxiway, taxiway_dir) = plane_dir
                            .to_owned()
                            .fetch_mappoint(&airport.map, plane.position)
                            .to_owned()
                            .check_for_taxiway(&airport.map, plane.position);
                        let mut pos = plane_dir.to_owned().go(plane.position);
                        if nearby_taxiway {
                            // Only stop if the direction is outward facing
                            // i.e. if we take that direction, and follow the path at that point,
                            // we should not end up on a runway
                            let mut outward_facing = false;
                            let potential_map_point = taxiway_dir
                                .to_owned()
                                .fetch_mappoint(&airport.map, plane.position);
                            let potential_point = taxiway_dir.go(plane.position);
                            if let MapPoint::Taxiway((_, dir)) = potential_map_point {
                                if let MapPoint::Runway(_) =
                                    dir.fetch_mappoint(&airport.map, potential_point)
                                {
                                    outward_facing = true;
                                }
                            }
                            if !outward_facing {
                                pos = potential_point;
                                plane.current_action = Action::HoldPosition;
                            }
                        }
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
                        MapPoint::Runway((_, dir)) => dir,
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
            Action::TaxiOntoRunway(_) => {
                let point = airport.map.map[plane.position.0][plane.position.1].clone();
                match point {
                    MapPoint::Taxiway((_, dir)) => plane.position = dir.go(plane.position),
                    MapPoint::Runway((name, dir)) => match name {
                        0 => plane.current_action = Action::TaxiOntoRunway(name),
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
                        let (is_nearby_gate, gate_dir) =
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
            Action::AtGate((_, ref mut atgate_action)) => {
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
fn detect_and_handle_collisions(airport: &mut Airport, score: &mut Score) {
    let fleet = airport.planes.clone();
    let mut crashed_planes = None;
    for (i, plane) in fleet.iter().enumerate() {
        for another_plane in fleet.iter().skip(i + 1) {
            if plane.position == another_plane.position
                && plane.id != another_plane.id
                && plane.out_of_map == false
                && another_plane.out_of_map == false
            {
                crashed_planes = Some((plane, another_plane));
                break;
            }
        }
    }

    // Take appropriate actions in response to collisions
    if crashed_planes.is_some() {
        let (plane1, plane2) = crashed_planes.unwrap();
        let mut stdout = stdout();
        let collision_message = format!(
            "🎧 Attention, Air Traffic Control, this is Ground Operations. \
            We have a Code 34 incident on the tarmac involving aircraft {} and {}. \
            Two aircraft have come into contact. \
            Emergency services have been alerted and are en route. \
            All ground movement is currently halted. \
            Please hold all departures and redirect incoming traffic to alternate taxiways. \
            We will update as more information becomes available. Over.",
            plane1.name, plane2.name
        );
        stdout.write_all(collision_message.as_bytes()).unwrap();

        score.crash += 1;
    }
}

// Function to handle ground staff alerts
fn _handle_ground_alerts(_airport: &mut Airport, _alert: _GroundAlert) {
    // Take appropriate actions in response to ground staff alerts
}

fn parse_user_input(
    command: String,
    planes: &Vec<Plane>,
    runways: &HashMap<String, Runway>,
    weather: &Weather,
) -> Result<Plane, String> {
    /*
        Language is:
        l <aircraft> <runway_number>        : Landing at runway X
        t <aircraft> <runway_number>        : Takeoff from runway X
        hp <aircraft>                       : Hold position
        p <aircraft>                        : Pushback
        tor <aircraft> <runway_number>      : Taxi onto runway X
        hs <aircraft> <runway_number>       : Hold short of runway X
        t2g <aircraft> <gate_number>        : Taxi to gate X

        TODO:
        t2t <aircraft> <terminal_number>    : Taxi to terminal X
    */
    let command = command.split_whitespace().collect::<Vec<_>>();
    if command.len() > 3 || command.len() < 2 {
        return Err("Wrong user input length.".to_string());
    }
    let keyword = command[0];
    let aircraft = command[1].to_string().to_lowercase();
    let mut plane = planes
        .iter()
        .find(|plane| plane.name.to_lowercase() == aircraft)
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
        "tor" => Action::TaxiOntoRunway(destination_num.clone().unwrap().parse::<usize>().unwrap()),
        "hs" => Action::HoldShort,
        "t2g" => Action::TaxiToGate(destination_num.clone().unwrap()),
        _ => Action::HoldPosition, // Should never happen
    };

    /*
        Valid successors for each action:
        InAir: -
        Land: -
        HoldPosition: TaxiToGate (after landing), TaxiToRunway, HoldShort, TaxiOntoRunway
        Pushback: -
        TaxiOntoRunway: HoldPosition, HoldShort, Takeoff, TaxiToRunway, TaxiToGate
        HoldShort: HoldPosition, TaxiOntoRunway, Takeoff, TaxiToRunway
        TaxiToGate: HoldPosition
        Takeoff: -
        AtGate: Pushback (only when on standby)
    */
    match plane.current_action {
        Action::InAir => return Err("Not a valid action when plane is in the air".to_string()),
        Action::Land => return Err("Not a valid action when in the process of landing".to_string()),
        Action::Takeoff => {
            return Err("Not a valid action when in the process of takeoff".to_string())
        }
        Action::HoldPosition => match action {
            Action::TaxiToGate(_) | Action::HoldShort | Action::TaxiOntoRunway(_) => {}
            _ => {
                return Err("Not a valid action when holding position".to_string());
            }
        },
        Action::TaxiOntoRunway(_) => match action {
            // Need TaxiToGate during emergency situations
            Action::HoldPosition | Action::HoldShort | Action::TaxiToGate(_) => {}
            Action::Takeoff => {
                if weather.condition == WeatherCondition::InclementWeather {
                    return Err(
                        "Cannot takeoff during inclement weather, return back to the gate"
                            .to_string(),
                    );
                }
            }
            _ => {
                return Err("Not a valid action when taxiing onto runway".to_string());
            }
        },
        Action::HoldShort => match action {
            Action::HoldPosition | Action::TaxiOntoRunway(_) => {}
            Action::Takeoff => {
                if weather.condition == WeatherCondition::InclementWeather {
                    return Err(
                        "Cannot takeoff during inclement weather, return back to the gate"
                            .to_string(),
                    );
                }
            }
            _ => {
                return Err("Not a valid action when holding short".to_string());
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
        Action::AtGate((_, at_gate_action)) => match action {
            Action::Pushback => {
                if at_gate_action != AtGateAction::Standby {
                    return Err("Wait for the plane to finish its turnaround process".to_string());
                }
                if weather.condition == WeatherCondition::InclementWeather {
                    return Err("Cannot pushback during inclement weather".to_string());
                }
            }
            _ => {
                return Err("Not a valid action when at gate".to_string());
            }
        },
    }

    plane.current_action = action;

    Ok(plane)
}

fn create_atc_clearance(airport: &Airport, plane: &Plane) -> String {
    let name = AIRWAY_IDS.get(plane.name.get(..2).unwrap()).unwrap();
    let code = plane.name.get(2..).unwrap().to_string();
    let clearance = match &plane.current_action {
        Action::Land => format!(
            "{} {}, you are cleared to land on runway {}.",
            name, code, plane.runway.name
        ),
        Action::Takeoff => {
            format!(
                "{} {}, you are cleared for takeoff, runway {}. Conditions {:.2} at {} knots.",
                name,
                code,
                plane.runway.name,
                airport.weather.wind_direction,
                airport.weather.wind_speed as usize
            )
        }
        Action::HoldPosition => format!("{} {}, hold position, traffic crossing.", name, code),
        Action::Pushback => format!(
            "{} {}, pushback approved, expect runway {} for departure.",
            name, code, plane.runway.name
        ),
        Action::TaxiOntoRunway(num) => {
            format!("{} {}, taxi directly to runway {}.", name, code, num)
        }
        Action::HoldShort => {
            format!(
                "{} {}, hold short of runway {} for landing traffic.",
                name, code, plane.runway.name
            )
        }
        Action::TaxiToGate(gate) => {
            // Find the taxiway closest to the plane's position
            let point: MapPoint = airport.map.map[plane.position.0][plane.position.1].clone();
            let taxiway = match point {
                MapPoint::Taxiway((num, _)) => num,
                MapPoint::Runway((_, dir)) => {
                    let next = dir.go(plane.position);
                    let next_point = airport.map.map[next.0][next.1].clone();
                    match next_point {
                        MapPoint::Taxiway((num, _)) => num,
                        _ => 0,
                    }
                }
                _ => 0,
            };
            match taxiway {
                0 => format!("{} {}, taxi to gate {}.", name, code, gate.clone()),
                _ => format!(
                    "{} {}, taxi to gate {} via taxiway {}.",
                    name,
                    code,
                    gate.clone(),
                    taxiway
                ),
            }
        }
        Action::InAir => "".to_string(),
        Action::AtGate(_) => "".to_string(),
    };
    clearance
}

fn update_score(airport: &mut Airport, score: &mut Score) {
    // Update the score based on the current game state
    let mut num_takeoffs = 0;
    for plane in airport.planes.iter() {
        if plane.out_of_map {
            num_takeoffs += 1;
        }
    }
    score.takeoff = num_takeoffs;
}

// Function to simulate weather conditions
fn simulate_weather(airport: &mut Airport) {
    let mut rng = rand::thread_rng();
    airport.weather.condition = match airport.weather.condition {
        WeatherCondition::Clear => {
            if rng.gen_range(0..300) <= 1 {
                WeatherCondition::Rain
            } else if rng.gen_range(0..1000) <= 1 {
                let inclement_weather = "⚠️  Airport Operations Center (AOC): \n\
                    Attention all passengers and crew, \
                    due to the current severe weather conditions, \
                    all departing flights have been temporarily halted for passenger safety. \
                    Incoming flights that are close to landing will proceed as scheduled. \
                    We appreciate your understanding and cooperation. \
                    Please stay tuned to the flight information displays \
                    and airport announcements for further updates. \
                    We sincerely apologize for any inconvenience caused. \
                    Your safety is our top priority. Thank you.";
                if let Ok(mut aoc) = AOC.lock() {
                    aoc.message = inclement_weather.to_owned();
                }
                WeatherCondition::InclementWeather
            } else {
                WeatherCondition::Clear
            }
        }
        WeatherCondition::Rain => {
            if rng.gen_range(0..100) < 95 {
                WeatherCondition::Rain
            } else {
                WeatherCondition::Clear
            }
        }
        WeatherCondition::InclementWeather => {
            if rng.gen_range(0..100) < 98 {
                WeatherCondition::InclementWeather
            } else {
                // No more inclement weather alert
                if let Ok(mut aoc) = AOC.lock() {
                    aoc.message = String::new();
                }
                WeatherCondition::Clear
            }
        }
    };
    simulate_wind_direction_and_speed(&mut airport.weather, 10);
}

fn simulate_wind_direction_and_speed(weather: &mut Weather, prob: usize) {
    let mut rng = rand::thread_rng();
    if rng.gen_range(0..100) < prob {
        weather.wind_speed = match weather.condition {
            WeatherCondition::Clear => {
                let normal = Normal::new(10.0, 1.0).unwrap();
                let mut s = normal.sample(&mut rand::thread_rng());
                s = if s < 0.0 && s > 20.0 { 20.0 } else { s };
                s
            }
            WeatherCondition::Rain => {
                let normal = Normal::new(30.0, 5.0).unwrap();
                let mut s = normal.sample(&mut rand::thread_rng());
                s = if s < 20.0 && s > 40.0 { 40.0 } else { s };
                s
            }
            WeatherCondition::InclementWeather => {
                let normal = Normal::new(50.0, 10.0).unwrap();
                let mut s = normal.sample(&mut rand::thread_rng());
                s = if s < 50.0 && s > 60.0 { 60.0 } else { s };
                s
            }
        };
    }

    if prob == 100 || rng.gen_range(0..100) < 5 {
        let normal_wind_direction = Normal::new(weather.wind_direction as f64, 20.0).unwrap();
        let dir = normal_wind_direction.sample(&mut rand::thread_rng());
        weather.wind_direction = if dir > 360.0 {
            f64::min(dir - 360.0, 360.0)
        } else if dir < 0.0 {
            f64::max(dir + 360.0, 0.0)
        } else {
            dir
        } as usize;
    }
}

fn spawn_landing_aircraft(airport: &mut Airport, at_gate: bool) {
    // Spawn new aircraft for landing
    let spacing = &airport.map.spacing;
    let runways = &airport.runways;
    let num_planes = airport.planes.len();

    let mut rng = rand::thread_rng();
    let airway_ids: Vec<_> = AIRWAY_IDS.keys().cloned().collect();
    let plane_name = airway_ids[rng.gen_range(0..airway_ids.len())].to_string()
        + &rng.gen_range(100..400).to_string();

    let (position, current_action) = match at_gate {
        true => {
            let random_gate = airport
                .gates
                .values()
                .collect::<Vec<_>>()
                .choose(&mut rand::thread_rng())
                .unwrap()
                .to_owned();
            (
                random_gate.position,
                Action::AtGate((random_gate.number.clone(), AtGateAction::Standby)),
            )
        }
        false => ((spacing.top_bottom, 0), Action::InAir),
    };

    let plane = Plane {
        id: num_planes + 1,
        name: plane_name,
        current_action,
        position,
        runway: runways["1"].clone(),
        out_of_map: false,
    };

    airport.planes.push(plane);
}

fn user_input_thread(sender: std::sync::mpsc::Sender<String>) {
    let stream = TcpStream::connect("localhost:8080").unwrap();
    let mut reader = BufReader::new(stream);
    loop {
        let mut user_input = String::new();
        reader
            .read_line(&mut user_input)
            .expect("Failed to read user input");

        // Trim whitespace and newline characters from the input
        user_input = user_input.trim().to_string();

        // Send the user input to the main game loop through the channel
        sender.send(user_input).expect("Failed to send user input");
    }
}

fn tcp_listener() {
    let listener = TcpListener::bind("localhost:8080").expect("Failed to bind address");
    for stream in listener.incoming() {
        let mut stream = stream.unwrap();
        let stdin = io::stdin();
        for line in stdin.lock().lines() {
            let line = line.unwrap();
            stream.write(line.as_bytes()).unwrap();
            stream.write(b"\n").unwrap();
            stream.flush().unwrap();
        }
    }
}

// Main function to run the game
fn main() {
    let args = Args::parse();
    if !args.sim {
        tcp_listener();
    }

    // Initialize and run your ATC game here
    let mut airport = construct_airport();
    let time: Time = Time { step_duration: 1 };
    const LANDING_INTERVAL: usize = 60;
    let mut score = Score {
        takeoff: 0,
        crash: 0,
    };

    // Channel for communication between threads
    let (sender, receiver): (std::sync::mpsc::Sender<String>, Receiver<String>) = channel();

    // Separate thread for handling user input
    std::thread::spawn(move || {
        user_input_thread(sender);
    });

    // TTS
    let mut tts = Tts::default().expect("Could not initialize TTS");

    // Spawn the first aircraft at a gate
    spawn_landing_aircraft(&mut airport, true);

    let mut timer: usize = 0;
    loop {
        let spawn_plane = timer % LANDING_INTERVAL == 0;
        update_game_state(&mut airport, spawn_plane, &mut score, &receiver, &mut tts);
        // Sleep for a bit
        thread::sleep(Duration::from_secs(time.step_duration as u64));
        timer += 1;
        if score.crash > 0 {
            break;
        }
    }
}
