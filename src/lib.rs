use std::{collections::VecDeque, fs, path::PathBuf};

use bevy::{
    asset::{AssetLoader, LoadedAsset},
    ecs::{schedule::ShouldRun, system::Command},
    prelude::*,
    reflect::TypeUuid,
    utils::HashMap,
};
use derive_deref::{Deref, DerefMut};
use nom::{
    bytes::complete::{is_not, take_until},
    character::complete::char,
    error::Error,
    sequence::{delimited, pair},
    Err,
};
use prost::Message;
pub use yharnam::*;

pub struct DialoguePlugin {
    pub startup_program: PathBuf,
}

impl Plugin for DialoguePlugin {
    fn build(&self, app: &mut App) {
        app.add_asset::<YarnProgram>()
            .add_asset::<YarnStringTable>()
            .init_asset_loader::<YarnProgramLoader>()
            .init_asset_loader::<YarnStringTableLoader>()
            .init_resource::<DialogueQueue>()
            .add_event::<EventDialogueUpdated>()
            .add_system_to_stage(CoreStage::PostUpdate, check_queue)
            .add_system_to_stage(CoreStage::PreUpdate, update_runner.with_run_criteria(run_if_no_dialogue_hold))
            .init_resource::<DialogueCommands>();

        let program_bytes = fs::read(self.startup_program.as_path()).unwrap();
        let program = Program::decode(&*program_bytes).unwrap();

        let mut csv_path = self.startup_program.clone();
        csv_path.set_extension("csv");
        let mut csv_reader = csv::Reader::from_path(csv_path).unwrap();
        let string_table: Vec<LineInfo> = csv_reader
            .deserialize()
            .map(|result| result.unwrap())
            .collect();
        app.insert_resource(DialogueRunner {
            vm: VirtualMachine::new(program),
            table: string_table,
            state: DialogueRunnerState::Idle,
        });
    }
}
pub trait RegisterDialogueCommandExt {
    fn register_dialogue_command<I: Into<String>>(
        &mut self,
        name: I,
        command: fn(&mut World, Vec<String>),
    ) -> &mut Self;
}

impl RegisterDialogueCommandExt for World {
    fn register_dialogue_command<I: Into<String>>(
        &mut self,
        name: I,
        command: fn(&mut World, Vec<String>),
    ) -> &mut Self {
        let mut commands = self.get_resource_or_insert_with(|| DialogueCommands::default());
        commands.insert(name.into(), command);
        self
    }
}

impl RegisterDialogueCommandExt for App {
    fn register_dialogue_command<I: Into<String>>(
        &mut self,
        name: I,
        command: fn(&mut World, Vec<String>),
    ) -> &mut Self {
        self.world.register_dialogue_command(name, command);
        self
    }
}
// *****************************************************************************************
// Events
// *****************************************************************************************
pub struct EventDialogueUpdated;
// *****************************************************************************************
// Resources
// *****************************************************************************************
#[derive(Default, Deref, DerefMut)]
pub struct DialogueQueue {
    pub queue: VecDeque<DialogueQueueEntry>,
}

pub struct DialogueQueueEntry {
    pub path: PathBuf,
    pub program: Handle<YarnProgram>,
    pub table: Handle<YarnStringTable>,
    pub start_node: Option<String>,
}

pub struct DialogueRunner {
    pub vm: VirtualMachine,
    pub table: Vec<LineInfo>,
    pub state: DialogueRunnerState,
}

#[derive(Debug, Clone)]
pub enum DialogueRunnerState {
    Idle,
    Running(DialogueRunningCurrentEntry),
}

#[derive(Debug, Clone)]
pub enum DialogueRunningCurrentEntry {
    Null,
    Text(String),
    Options(Vec<String>),
}

impl DialogueRunner {
    fn setup(&mut self, program: YarnProgram, table: YarnStringTable, start_node: Option<String>) {
        let start_node = match start_node {
            Some(s) => s.clone(),
            None => "Start".to_string(),
        };
        self.vm.program = program.0;
        self.table = table.0;
        //println!("Nodes: {:?}", vm.program.nodes);
        if self.vm.program.nodes.contains_key(&start_node) {
            // Set the start node.
            //println!("Start node set!");
            self.vm.set_node(&start_node);
        }
        self.state = DialogueRunnerState::Running(DialogueRunningCurrentEntry::Null);
    }
}

impl PartialEq for DialogueRunnerState {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (DialogueRunnerState::Idle, DialogueRunnerState::Idle) => true,
            (DialogueRunnerState::Idle, DialogueRunnerState::Running { .. }) => false,
            (DialogueRunnerState::Running { .. }, DialogueRunnerState::Idle) => false,
            (DialogueRunnerState::Running { .. }, DialogueRunnerState::Running { .. }) => true,
        }
    }
}

#[derive(Deref, DerefMut, Default)]
pub struct DialogueCommands(HashMap<String, fn(&mut World, Vec<String>)>);

pub struct DialogueHold;
// *****************************************************************************************
// Systems
// *****************************************************************************************
fn check_queue(
    mut queue: ResMut<DialogueQueue>,
    mut runner: ResMut<DialogueRunner>,
    mut yarn_programs: ResMut<Assets<YarnProgram>>,
    mut yarn_tables: ResMut<Assets<YarnStringTable>>,
) {
    if runner.state == DialogueRunnerState::Idle && !queue.is_empty() {
        //println!("Setting up runner");

        let temp_entry = queue.get(0).unwrap();
        if yarn_programs.get(&temp_entry.program).is_some()
            && yarn_tables.get(&temp_entry.table).is_some()
        {
            let entry = queue
                .pop_front()
                .expect("setup_runner: Dialogue queue empty!");

            if let Some(program) = yarn_programs.remove(entry.program) {
                //println!("Program Valid!");
                if let Some(table) = yarn_tables.remove(entry.table) {
                    runner.setup(program, table, entry.start_node)
                }
            } else {
                //println!("Program not ready yet!");
            }
        } else {
            //println!("Program not ready yet!");
        }
    }
}

fn update_runner(
    mut commands: Commands,
    mut runner: ResMut<DialogueRunner>,
    mut yarn_tables: ResMut<Assets<YarnStringTable>>,
    mut queue: ResMut<DialogueQueue>,
    mut yarn_programs: ResMut<Assets<YarnProgram>>,
    mut event_writer: EventWriter<EventDialogueUpdated>,
) {
    if let DialogueRunnerState::Running(..) = runner.state.clone() {
        let next_selection = match runner.vm.execution_state {
            ExecutionState::WaitingOnOptionSelection => return,
            _ => {
                match runner.vm.continue_dialogue() {
                    SuspendReason::Line(line) => {
                        let new_text = runner.table.iter()
                        .find(|line_info| line_info.id == line.id)
                        .map(|line_info| &line_info.text)
                        ;

                        if let Some(new_text) = new_text {
                            let subs = substitute(new_text.as_str(), &line.substitutions);
                            event_writer.send(EventDialogueUpdated);
                            DialogueRunningCurrentEntry::Text(subs)
                        }
                        else {
                            panic!("Error! unable to find line!");
                        }
                    }
                    SuspendReason::Options(new_options) => {
                        let mut o = Vec::new();
                        for opt in new_options.iter() {
                            let t = runner.table.iter()
                                .find(|line_info| line_info.id == opt.line.id)
                                .map(|line_info| &line_info.text)
                            ;
                            if let Some(t) = t {
                                o.push(t.clone());
                            }
                        }
                        event_writer.send(EventDialogueUpdated);
                        DialogueRunningCurrentEntry::Options(o)
                    }
                    SuspendReason::Command(command_text) => {
                        //println!("== Command: {} ==", command_text);
                        let mut arguments: Vec<String> = command_text.split(" ").map(|s| {s.to_string()}).collect()
                        ;
                        if !arguments.is_empty() {
                            let name = arguments.remove(0);
                            commands.add(ExecuteDialogueCommand {
                                command: name, 
                                args: arguments,
                            });
                        }
                        DialogueRunningCurrentEntry::Null
                    },
                    SuspendReason::NodeChange { .. } => {
                        DialogueRunningCurrentEntry::Null
                        //println!("== Node end: {} ==", end);
                        //println!("== Node start: {} ==", start);
                    },
                    SuspendReason::DialogueComplete(_last_node) => {
                        //println!("== Node end: {} ==", last_node);
                        //println!("== Dialogue complete ==");
                        match queue.pop_front() {
                            Some(entry) => {
                                if yarn_programs.get(&entry.program).is_some() && yarn_tables.get(&entry.table).is_some() {
                                    if let Some(program) = yarn_programs.remove(entry.program) {
                                        if let Some(table) = yarn_tables.remove(entry.table) {
                                            runner.setup(program, table, entry.start_node)
                                        }
                                    }
                                } else {
                                    runner.state = DialogueRunnerState::Idle;
                                }
                            }
                            None => runner.state = DialogueRunnerState::Idle,
                        }
                        return
                    }
                }
            }
        };

        runner.state = DialogueRunnerState::Running(next_selection);
    }
}

// *****************************************************************************************
// Asset Loaders
// *****************************************************************************************
#[derive(Debug, TypeUuid, Deref)]
#[uuid = "aa134e2e-a11e-4350-ae1e-b5410d0c333c"]
pub struct YarnStringTable(pub Vec<LineInfo>);

#[derive(Default)]
pub struct YarnStringTableLoader;

impl AssetLoader for YarnStringTableLoader {
    fn load<'a>(
        &'a self,
        bytes: &'a [u8],
        load_context: &'a mut bevy::asset::LoadContext,
    ) -> bevy::asset::BoxedFuture<'a, Result<(), anyhow::Error>> {
        Box::pin(async move {
            let mut csv_reader = csv::Reader::from_reader(bytes);
            let string_table: Vec<LineInfo> = csv_reader
                .deserialize()
                .map(|result| result.unwrap())
                .collect();

            load_context.set_default_asset(LoadedAsset::new(YarnStringTable(string_table)));
            Ok(())
        })
    }

    fn extensions(&self) -> &[&str] {
        &["csv"]
    }
}

#[derive(Debug, TypeUuid)]
#[uuid = "35d03e10-93b3-436e-8df4-7c7bea467dc0"]
pub struct YarnProgram(Program);
#[derive(Default)]
pub struct YarnProgramLoader;

impl AssetLoader for YarnProgramLoader {
    fn load<'a>(
        &'a self,
        bytes: &'a [u8],
        load_context: &'a mut bevy::asset::LoadContext,
    ) -> bevy::asset::BoxedFuture<'a, Result<(), anyhow::Error>> {
        Box::pin(async move {
            let program = Program::decode(bytes).unwrap();
            load_context.set_default_asset(LoadedAsset::new(YarnProgram(program)));
            Ok(())
        })
    }

    fn extensions(&self) -> &[&str] {
        &["yarnc"]
    }
}

pub struct AddDialogueToQueueCommand {
    pub path: PathBuf,
    pub start_node: Option<String>,
}

impl Command for AddDialogueToQueueCommand {
    fn write(self, world: &mut World) {
        let asset_server = world.get_resource::<AssetServer>().unwrap();

        let program = asset_server.load(self.path.as_path());
        let mut table_path = self.path.clone();
        table_path.set_extension("csv");
        let table = asset_server.load(table_path);

        let mut dialogue_queue = world.get_resource_mut::<DialogueQueue>().unwrap();
        dialogue_queue.push_back(DialogueQueueEntry {
            path: self.path.clone(),
            program,
            table,
            start_node: self.start_node,
        })
    }
}

fn substitute(input: &str, substitutions: &Vec<String>) -> String {
    let mut parser = pair(
        take_until("{"),
        delimited(char('{'), is_not("}"), char('}')),
    );

    //let mut parser = pair(is_not("["), delimited(char('['), is_not("]"), char(']')));
    let mut return_string = "".to_string();
    let mut remainder = input.to_string();
    let mut result: Result<(&str, (&str, &str)), Err<Error<&str>>> = parser(&input);

    if result.is_ok() {
        let mut idx: usize = 0;

        while result.is_ok() {
            if let Ok((remaining, (first, _sub))) = result {
                return_string.push_str(first);

                return_string.push_str(&substitutions.get(idx).unwrap().to_string());
                idx += 1;
                remainder = remaining.to_string();
                result = parser(remaining);
                if let Err(_err) = &result {
                    //println!("Failed to find more arguments. Error: {:?}, Remainder: {}", err, remainder);
                    return_string.push_str(&remainder);
                }
            }
        }
    } else {
        return_string = remainder.to_string();
    }

    return_string
}

// *****************************************************************************************
// Run Conditions
// *****************************************************************************************
pub fn run_if_dialogue_queue_occupied(queue: Res<DialogueQueue>) -> ShouldRun {
    match !queue.is_empty() {
        true => ShouldRun::Yes,
        false => ShouldRun::No,
    }
}

pub fn run_if_no_dialogue_hold(hold: Option<Res<DialogueHold>>) -> ShouldRun {
    match hold {
        Some(_) => ShouldRun::No,
        None => ShouldRun::Yes,
    }
}

pub fn run_if_dialogue_running(runner: Res<DialogueRunner>) -> ShouldRun {
    match runner.state {
        DialogueRunnerState::Idle => ShouldRun::No,
        DialogueRunnerState::Running { .. } => ShouldRun::Yes,
    }
}

pub struct ExecuteDialogueCommand {
    pub command: String,
    pub args: Vec<String>,
}

impl Command for ExecuteDialogueCommand {
    fn write(self, world: &mut World) {
        world.resource_scope(|world, command_registry: Mut<DialogueCommands>| {
            if let Some(com) = command_registry.0.get(&self.command) {
                com(world, self.args);
            }
        });
    }
}