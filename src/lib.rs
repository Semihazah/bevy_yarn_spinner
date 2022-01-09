use std::marker::PhantomData;

use bevy::{
    ecs::{
        component::Component,
        entity::{EntityMap, MapEntities, MapEntitiesError},
        reflect::ReflectMapEntities,
        system::{Command, EntityCommands},
        world::EntityMut,
    },
    prelude::*, reflect::FromReflect,
};

#[derive(Reflect, FromReflect, Clone, Component)]
#[reflect(Component)]
pub struct SyncData<T: Send + Sync + 'static, G: GiveData<T>, R: RecieveData<T>> {
    pub source: Entity,

    #[reflect(ignore)]
    phantom_data: PhantomData<T>,

    #[reflect(ignore)]
    phantom_giver: PhantomData<G>,

    #[reflect(ignore)]
    phantom_reciever: PhantomData<R>,
}

impl<T: Send + Sync + 'static, G: GiveData<T>, R: RecieveData<T>> SyncData<T, G, R> {
    pub fn new(source: Entity) -> Self {
        SyncData {
            source,
            phantom_data: PhantomData,
            phantom_giver: PhantomData,
            phantom_reciever: PhantomData,
        }
    }
}

impl<T: Send + Sync + 'static, G: GiveData<T>, R: RecieveData<T>> Default for SyncData<T, G, R> {
    fn default() -> Self {
        SyncData {
            source: Entity::from_raw(u32::MAX),
            phantom_data: PhantomData,
            phantom_giver: PhantomData,
            phantom_reciever: PhantomData,
        }
    }
}

impl<T: Send + Sync + 'static, G: GiveData<T>, R: RecieveData<T>> MapEntities
    for SyncData<T, G, R>
{
    fn map_entities(&mut self, m: &EntityMap) -> Result<(), MapEntitiesError> {
        self.source = m.get(self.source).unwrap();

        Ok(())
    }
}

pub trait RecieveData<T: Send + Sync + 'static>: Component {
    fn recieve_data<I: Into<T>>(
        &mut self,
        data: I,
        reflect_data: &dyn Reflect,
        asset_server: &Res<AssetServer>,
    );
}

pub trait GiveData<T: Send + Sync + 'static>: Component + FromWorld + Reflect {
    fn give_data(&self) -> T;
}

#[derive(Reflect, FromReflect, Clone, Component)]
#[reflect(Component, MapEntities)]
pub struct GiveList<T: Send + Sync + 'static, G: GiveData<T>, R: RecieveData<T>> {
    pub recievers: Vec<Entity>,

    #[reflect(ignore)]
    phantom_data: PhantomData<T>,

    #[reflect(ignore)]
    phantom_giver: PhantomData<G>,

    #[reflect(ignore)]
    phantom_reciever: PhantomData<R>,
}

impl<T: Send + Sync + 'static, G: GiveData<T>, R: RecieveData<T>> GiveList<T, G, R> {
    pub fn new(list: Vec<Entity>) -> Self {
        GiveList {
            recievers: list,
            phantom_data: PhantomData,
            phantom_giver: PhantomData,
            phantom_reciever: PhantomData,
        }
    }
}
impl<T: Send + Sync + 'static, G: GiveData<T>, R: RecieveData<T>> Default for GiveList<T, G, R> {
    fn default() -> Self {
        GiveList {
            recievers: Vec::default(),
            phantom_data: PhantomData,
            phantom_giver: PhantomData,
            phantom_reciever: PhantomData,
        }
    }
}
impl<T: Send + Sync + 'static, G: GiveData<T>, R: RecieveData<T>> MapEntities
    for GiveList<T, G, R>
{
    fn map_entities(&mut self, m: &EntityMap) -> Result<(), MapEntitiesError> {
        for reciever in self.recievers.iter_mut() {
            *reciever = m.get(*reciever).unwrap();
        }

        Ok(())
    }
}

pub struct SyncToDataCommand<T: Send + Sync + 'static, G: GiveData<T>, R: RecieveData<T>> {
    pub entity: Entity,
    pub source: Entity,
    phantom_data: PhantomData<T>,
    phantom_giver: PhantomData<G>,
    phantom_reciever: PhantomData<R>,
}

impl<T: Send + Sync + 'static, G: GiveData<T>, R: RecieveData<T>> Command
    for SyncToDataCommand<T, G, R>
{
    fn write(self: Self, world: &mut World) {
        world
            .entity_mut(self.entity)
            .insert(SyncData::<T, G, R>::new(self.source));
        match world.entity(self.source).contains::<GiveList<T, G, R>>() {
            false => {
                world
                    .entity_mut(self.source)
                    .insert(GiveList::<T, G, R>::new(vec![self.entity]));
            }
            true => {
                let mut g = world
                    .entity_mut(self.source)
                    .get_mut::<GiveList<T, G, R>>()
                    .unwrap();
                g.recievers.push(self.entity);
            }
        }
    }
}

pub trait SyncToDataCommandExt {
    fn sync_to_data<T: Send + Sync + 'static, G: GiveData<T>, R: RecieveData<T>>(
        &mut self,
        source: Entity,
    ) -> &mut Self;
}

impl<'w, 's, 'a> SyncToDataCommandExt for EntityCommands<'w, 's, 'a> {
    fn sync_to_data<T: Send + Sync + 'static, G: GiveData<T>, R: RecieveData<T>>(
        &mut self,
        source: Entity,
    ) -> &mut Self {
        let id = self.id();

        self.commands().add(SyncToDataCommand::<T, G, R> {
            entity: id,
            source,
            phantom_data: PhantomData,
            phantom_giver: PhantomData,
            phantom_reciever: PhantomData,
        });

        self
    }
}

impl<'w> SyncToDataCommandExt for EntityMut<'w> {
    fn sync_to_data<T: Send + Sync + 'static, G: GiveData<T>, R: RecieveData<T>>(
        &mut self,
        source: Entity,
    ) -> &mut Self {
        let id = self.id();
        unsafe {
            self.world_mut()
                .entity_mut(id)
                .insert(SyncData::<T, G, R>::new(source));
            self.update_location();
        }
        match self.world().entity(source).contains::<GiveList<T, G, R>>() {
            false => unsafe {
                self.world_mut()
                    .entity_mut(source)
                    .insert(GiveList::<T, G, R>::new(vec![id]));
                self.update_location();
            },
            true => unsafe {
                let mut g = self
                    .world_mut()
                    .entity_mut(source)
                    .get_mut::<GiveList<T, G, R>>()
                    .unwrap();
                g.recievers.push(id);
            },
        }

        self
    }
}

pub fn sync_data<T: Send + Sync + 'static, G: GiveData<T>, R: RecieveData<T>>(
    asset_server: Res<AssetServer>,
    mut give_query: Query<
        (&G, &mut GiveList<T, G, R>),
        Or<(Changed<G>, Changed<GiveList<T, G, R>>)>,
    >,
    mut recieve_query: Query<&mut R>,
) {
    for (data, mut list) in give_query.iter_mut() {
        let mut remove_list = Vec::new();
        for recieve_entity in list.recievers.iter() {
            //println!("Syncing changed data for types {}, {}, {}, reciever = {:?}", type_name::<T>(), type_name::<G>(), type_name::<R>(), recieve_entity);
            if let Ok(mut reciever) = recieve_query.get_mut(*recieve_entity) {
                //println!("Sync data success!");
                reciever.recieve_data(data.give_data(), data as &dyn Reflect, &asset_server);
            } else {
                //println!("Sync data failed! Could not find reciever!");

                remove_list.push(*recieve_entity);
            }
        }

        list.recievers
            .retain(|entity| !remove_list.contains(entity))
    }
}

pub fn sync_init_data<T: Send + Sync + 'static, G: GiveData<T>, R: RecieveData<T>>(
    asset_server: Res<AssetServer>,
    mut recieve_query: Query<(&mut R, &SyncData<T, G, R>), Changed<SyncData<T, G, R>>>,
    give_query: Query<&G>,
) {
    for (mut reciever, sync) in recieve_query.iter_mut() {
        //println!("Syncing init data for types {}, {}, {}", type_name::<T>(), type_name::<G>(), type_name::<R>());
        if let Ok(giver) = give_query.get(sync.source) {
            //println!("Found giver!");
            reciever.recieve_data(giver.give_data(), giver as &dyn Reflect, &asset_server);
        }
    }
}
pub trait SyncBuilder {
    fn register_data_sync<T, G, R>(&mut self) -> &mut Self
    where
        T: Send + Sync + 'static,
        G: GiveData<T>,
        R: RecieveData<T>;
}

impl SyncBuilder for App {
    fn register_data_sync<T, G, R>(&mut self) -> &mut Self
    where
        T: Send + Sync + 'static,
        G: GiveData<T>,
        R: RecieveData<T>,
    {
        self.register_type::<SyncData<T, G, R>>()
            .register_type::<GiveList<T, G, R>>()
            .add_system_to_stage(
                CoreStage::PostUpdate,
                sync_data::<T, G, R>.system().label("sync_data"),
            )
            .add_system_to_stage(
                CoreStage::PostUpdate,
                sync_init_data::<T, G, R>.system().label("sync_data"),
            );

        self
    }
}