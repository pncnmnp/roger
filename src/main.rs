#[derive(Clone, PartialEq)]
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
}

struct Runway {
    name: usize,
    length: usize,
    side: Direction,
}

impl Runway {
    pub fn new(self) -> Self {
        Self {
            name: self.name,
            length: self.length,
            side: self.side,
        }
    }
}

struct Taxiway {
    name: usize,
    length: usize,
    width: usize,
}

impl Taxiway {
    pub fn new(self) -> Self {
        Self {
            name: self.name,
            length: self.length,
            width: self.width,
        }
    }
}

struct Terminal {
    name: char,
    gates: Vec<Gate>,
}

#[derive(Clone)]
struct Gate {
    number: String,
    is_occupied: bool,
}

struct TerminalComplex {
    num_terms: u8,
    gates_per_term: usize,
}

impl TerminalComplex {
    pub fn new(self) -> Vec<Terminal> {
        let mut terminals = vec![];
        for term_num in 0..self.num_terms {
            let mut terminal = Terminal {
                name: (term_num + b'A' as u8 - 1) as char,
                gates: vec![],
            };
            for gate_num in 0..self.gates_per_term {
                terminal.gates.push(Gate {
                    number: String::from(terminal.name) + &gate_num.to_string(),
                    is_occupied: false,
                });
            }
            terminals.push(terminal);
        }
        terminals
    }
}

#[derive(Clone, PartialEq)]
enum MapPoint {
    Runway((usize, Direction)),
    Taxiway((usize, Direction)),
    Gate(String),
    Plane(String),
    GateTaxiLine,
    Empty,
}

struct Map {
    length: usize,
    width: usize,
    spacing: usize,
    map: Vec<Vec<MapPoint>>,
}

enum WeatherCondition {
    Clear,
    Rain,
    InclementWeather,
}

enum Action {
    InAir,
    Land,
    Takeoff,
    HoldPosition,
    TaxiOntoRunway,
    HoldShort,
    TaxiToRunway(usize),
    TaxiToTerminal(String),
    TaxiToGate(String),
    Pushback,
    AtGate(String),
}

struct Plane {
    id: usize,
    name: String,
    current_action: Action,
    position: (usize, usize),
}

impl Plane {
    pub fn new(self) -> Self {
        Self {
            id: self.id,
            name: self.name,
            current_action: self.current_action,
            position: self.position,
        }
    }
}

struct Airport {
    runway: Runway,
    taxiway: Taxiway,
    terminals: Vec<Terminal>,
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
    let (x, y) = (160, 25);
    let spacing = 5;

    let runway = Runway {
        name: 1,
        side: Direction::West,
        length: 100,
    };
    let taxiway = Taxiway {
        length: 100,
        width: 100,
        name: 7,
    };
    let terminal_complex = TerminalComplex {
        num_terms: 2,
        gates_per_term: 3,
    };
    let planes = vec![Plane {
        id: 1,
        name: "AA117".to_owned(),
        current_action: Action::AtGate("A2".to_owned()),
        position: (0, 0),
    }];

    let runway = runway.new();
    let taxiway = taxiway.new();
    let terminals = terminal_complex.new();
    let map = build_airport_map(x, y, spacing, &runway, &taxiway, &terminals);
    let weather = WeatherCondition::Clear;

    Airport {
        runway,
        taxiway,
        terminals,
        map,
        weather,
        planes,
    }
}

fn build_airport_map(
    length: usize,
    width: usize,
    spacing: usize,
    runway: &Runway,
    taxiway: &Taxiway,
    terminals: &Vec<Terminal>,
) -> Map {
    let mut map: Vec<Vec<MapPoint>> = vec![vec![MapPoint::Empty; width]; length];

    let (run_startx, run_starty, run_endx, run_endy) =
        (spacing, spacing, spacing, runway.length + spacing);
    for i in run_starty..run_endy {
        for j in run_startx..run_endx {
            map[i][j] = MapPoint::Runway((runway.name, runway.side.clone()));
        }
    }
    // Connect the runway to the taxiway
    // TODO: Make this more general
    map[run_endx][run_endy] = MapPoint::Runway((runway.name, Direction::South));

    let taxiway_side = runway.side.clone().get_opposite_dir();
    let (taxi_startx, taxi_starty, taxi_endx, taxi_endy) = (
        taxiway.width + spacing,
        spacing,
        taxiway.width + spacing,
        taxiway.length + spacing,
    );
    for i in taxi_starty..taxi_endy {
        for j in taxi_startx..taxi_endx {
            map[i][j] = MapPoint::Taxiway((taxiway.name, taxiway_side.clone()));
        }
    }

    let term_startx = taxiway.width + (2 * spacing);
    let gates: Vec<Gate> = terminals
        .iter()
        .flat_map(|terminal| terminal.gates.iter())
        .cloned()
        .collect();
    let total_gates = gates.len();
    let term_spacing = taxiway.length / total_gates;

    for i in 0..total_gates {
        let y = spacing + (total_gates * term_spacing);
        map[term_startx][y] = MapPoint::Gate(gates[i].number.clone());
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
                let pos = match airport.runway.side {
                    Direction::West => {
                        plane_dir = Direction::East;
                        plane_dir.to_owned().go(plane.position)
                    }
                    Direction::East => {
                        plane_dir = Direction::West;
                        plane_dir.to_owned().go(plane.position)
                    }
                    Direction::North => todo!(),
                    Direction::South => todo!(),
                    Direction::StayPut => todo!(),
                };
                plane.position = pos;

                // Check if plane has reached the start of the runway
                let runway_name = airport.runway.name;
                if Direction::StayPut.fetch_mappoint(&airport.map, plane.position)
                    == MapPoint::Runway((runway_name, plane_dir))
                {
                    plane.current_action = Action::Land;
                }
            }
            Action::Land => {
                let pos = match airport.runway.side {
                    Direction::West => {
                        let pos = Direction::East.go(plane.position);
                        // Check if plane has reached the end of the runway
                        if Direction::East.fetch_mappoint(&airport.map, pos) == MapPoint::Empty {
                            plane.current_action = Action::HoldPosition;
                        }
                        pos
                    }
                    Direction::East => {
                        let pos = Direction::West.go(plane.position);
                        // Check if plane has reached the end of the runway
                        if Direction::West.fetch_mappoint(&airport.map, pos) == MapPoint::Empty {
                            plane.current_action = Action::HoldPosition;
                        }
                        pos
                    }
                    Direction::North => todo!(),
                    Direction::South => todo!(),
                    Direction::StayPut => todo!(),
                };
                plane.position = pos;
            }
            Action::Takeoff => {}
            Action::HoldPosition => {}
            Action::TaxiOntoRunway => {}
            Action::HoldShort => {}
            Action::TaxiToRunway(_) => {}
            Action::TaxiToTerminal(_) => {}
            Action::TaxiToGate(gate) => {
                // Check if the plane is at the end of the runway
                // If yes, then go to the taxiway

                // let runway_name = airport.runway.name;
                // if Direction::StayPut.fetch_mappoint(&airport.map, plane.position)
                //     == MapPoint::Runway(runway_name)
                // {
                //     plane.position =
                // }

                // Check if the plane is at the taxiway
                // If yes, then go to the gate

                // Check if the plane is at the gate
                // If yes, then go to the air bridge
            }
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
    let time = Time { step_duration: 3 };
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
        update_game_state(&mut airport, &time, &scheduling, &score);
        render(&mut airport);
    }
}
