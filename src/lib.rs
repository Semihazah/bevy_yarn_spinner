use std::{collections::VecDeque, path::PathBuf};

use bevy::{
    asset::{AssetLoader, LoadedAsset},
    ecs::{schedule::ShouldRun, system::Command},
    prelude::*,
    reflect::TypeUuid,
    utils::HashMap,
};
use derive_deref::{Deref, DerefMut};
use prost::Message;
use yharnam::*;

pub mod yarn_proto {
    include!(concat!(env!("OUT_DIR"), "/yarn.rs"));
}

pub struct DialoguePlugin;

impl Plugin for DialoguePlugin {
    fn build(&self, app: &mut App) {
        app.add_asset::<YarnProgram>()
            .add_asset::<YarnStringTable>()
            .init_asset_loader::<YarnProgramLoader>()
            .init_asset_loader::<YarnStringTableLoader>()
            .insert_resource(DialogueRunner::Idle)
            .init_resource::<DialogueQueue>()
            .add_system_to_stage(CoreStage::PreUpdate, check_queue.system())
            .add_system_to_stage(CoreStage::PostUpdate, update_runner.exclusive_system())
            .init_resource::<DialogueCommands>();
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

pub enum DialogueRunner {
    Idle,
    Running {
        vm: VirtualMachine,
        text: String,
        table: YarnStringTable,
        options: Option<Vec<String>>,
    },
}

impl DialogueRunner {
    fn setup(&mut self, program: YarnProgram, table: YarnStringTable, start_node: Option<String>) {
        let start_node = match start_node {
            Some(s) => s.clone(),
            None => "Start".to_string(),
        };
        let mut vm = VirtualMachine::new(program.0.clone());
        //println!("Nodes: {:?}", vm.program.nodes);
        if vm.program.nodes.contains_key(&start_node) {
            // Set the start node.
            println!("Start node set!");
            vm.set_node(&start_node);
        }
        *self = DialogueRunner::Running {
            vm,
            text: "".to_string(),
            options: None,
            table,
        };
    }
}

impl PartialEq for DialogueRunner {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (DialogueRunner::Idle, DialogueRunner::Idle) => true,
            (DialogueRunner::Idle, DialogueRunner::Running { .. }) => false,
            (DialogueRunner::Running { .. }, DialogueRunner::Idle) => false,
            (DialogueRunner::Running { .. }, DialogueRunner::Running { .. }) => true,
        }
    }
}

#[derive(Deref, DerefMut, Default)]
pub struct DialogueCommands(HashMap<String, fn(&mut World, Vec<String>)>);
// *****************************************************************************************
// Systems
// *****************************************************************************************
fn check_queue(
    mut queue: ResMut<DialogueQueue>,
    mut runner: ResMut<DialogueRunner>,
    mut yarn_programs: ResMut<Assets<YarnProgram>>,
    mut yarn_tables: ResMut<Assets<YarnStringTable>>,
) {
    if *runner == DialogueRunner::Idle && !queue.is_empty() {
        println!("Setting up runner");
        let entry = queue
            .pop_front()
            .expect("setup_runner: Dialogue queue empty!");

        if yarn_programs.get(&entry.program).is_some() && yarn_tables.get(&entry.table).is_some() {
            if let Some(program) = yarn_programs.remove(entry.program) {
                println!("Program Valid!");
                if let Some(table) = yarn_tables.remove(entry.table) {
                    runner.setup(program, table, entry.start_node)
                }
            } else {
                println!("Program not ready yet!");
            }
        }
    }
}

pub fn run_if_dialogue_queue_occupied(queue: Res<DialogueQueue>) -> ShouldRun {
    match !queue.is_empty() {
        true => ShouldRun::Yes,
        false => ShouldRun::No,
    }
}

fn update_runner(
    world: &mut World,
    /*     mut runner: ResMut<DialogueRunner>,
    mut yarn_tables: ResMut<Assets<YarnStringTable>>,
    mut queue: ResMut<DialogueQueue>,
    mut yarn_programs: ResMut<Assets<YarnProgram>>,
    dialogue_commands: Res<DialogueCommands>, */
) {
    world.resource_scope(|world, mut runner: Mut<DialogueRunner>| {
        match *runner {
            DialogueRunner::Idle => {
                return
            },
            DialogueRunner::Running {
                ref mut vm,
                ref mut text,
                ref mut options,
                ref table,
            } => {
                match vm.execution_state {
                    ExecutionState::WaitingOnOptionSelection => return,
                    _ => {
                        match vm.continue_dialogue() {
                            SuspendReason::Line(line) => {
                                *options = None;
                                let new_text = table.iter()
                                .find(|line_info| line_info.id == line.id)
                                .map(|line_info| &line_info.text)
                                ;
                                if let Some(new_text) = new_text {
                                    *text = new_text.clone();
                                }
                                else {
                                    panic!("Error! unable to find line!");
                                }
                            }
                            SuspendReason::Options(new_options) => {
                                let mut o = Vec::new();
                                for opt in new_options.iter() {
                                    let text = table.iter()
                                        .find(|line_info| line_info.id == opt.line.id)
                                        .map(|line_info| &line_info.text)
                                    ;
                                    if let Some(text) = text {
                                        o.push(text.clone());
                                    }
                                }
                                *options = Some(o);
                            }
                            SuspendReason::Command(command_text) => {
                                println!("== Command: {} ==", command_text);
                                let mut arguments: Vec<String> = command_text.split(" ").map(|s| {s.to_string()}).collect()
                                ;
                                if !arguments.is_empty() {
                                    let name = arguments.remove(0);
                                    world.resource_scope(|world, dialogue_commands: Mut<DialogueCommands>| {
                                        if let Some(com) = dialogue_commands.get(&name) {
                                            com(world, arguments);
                                        }
                                    });
                                }
                            },
                            SuspendReason::NodeChange { start, end } => {
                                println!("== Node end: {} ==", end);
                                println!("== Node start: {} ==", start);
                            },
                            SuspendReason::DialogueComplete(last_node) => {
                                println!("== Node end: {} ==", last_node);
                                println!("== Dialogue complete ==");
                                world.resource_scope(|world, mut queue: Mut<DialogueQueue>| {
                                    match queue.pop_front() {
                                        Some(entry) => {
                                            world.resource_scope(|world, mut yarn_programs: Mut<Assets<YarnProgram>>| {
                                                world.resource_scope(|_world, mut yarn_tables: Mut<Assets<YarnStringTable>>| {
                                                    if yarn_programs.get(&entry.program).is_some() && yarn_tables.get(&entry.table).is_some() {
                                                        if let Some(program) = yarn_programs.remove(entry.program) {
                                                            if let Some(table) = yarn_tables.remove(entry.table) {
                                                                runner.setup(program, table, entry.start_node)
                                                            }
                                                        }
                                                    } else {
                                                        *runner = DialogueRunner::Idle;
                                                    }
                                                });
                                            });
                                        }
                                        None => *runner = DialogueRunner::Idle,
                                    }
                                })
                            }
                        }
                    }
                }
            }
        }
    });
}

pub fn run_if_dialogue_running(runner: Res<DialogueRunner>) -> ShouldRun {
    match *runner {
        DialogueRunner::Idle => ShouldRun::No,
        DialogueRunner::Running { .. } => ShouldRun::Yes,
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
