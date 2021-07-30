use dashmap::DashMap;
use once_cell::sync::OnceCell;
use openxr::Result;
use openxr::sys as xr;
use openxr::sys::pfn as pfn;

use std::collections::HashMap;
use std::ffi::CString;
use std::ops::Add;
use std::ptr;
use std::sync::RwLock;
use std::sync::Weak;
use std::sync::Arc;

use crate::god_actions::ActionState;
use crate::god_actions::SubactionCollection;

type HandleMap<H, T> = DashMap<H, Arc<T>>;
type HandleRef<'a, H, T> = dashmap::mapref::one::Ref<'a, H, Arc<T>>;

static mut INSTANCES:   Option<HandleMap<xr::Instance, InstanceWrapper>> = None;
static mut SESSIONS:    Option<HandleMap<xr::Session, SessionWrapper>> = None;
static mut ACTIONS:     Option<HandleMap<xr::Action, ActionWrapper>> = None;
static mut ACTION_SETS: Option<HandleMap<xr::ActionSet, ActionSetWrapper>> = None;

pub unsafe fn static_init() {
    if INSTANCES.is_none() {
        INSTANCES = Some(DashMap::new());
        SESSIONS = Some(DashMap::new());
        ACTIONS = Some(DashMap::new());
        ACTION_SETS = Some(DashMap::new());
    }
}

pub fn instances() -> &'static HandleMap<xr::Instance, InstanceWrapper> {
    unsafe {
        INSTANCES.as_ref().unwrap()
    }
}

pub fn sessions() -> &'static HandleMap<xr::Session, SessionWrapper> {
    unsafe {
        SESSIONS.as_ref().unwrap()
    }
}

pub fn action_sets() -> &'static HandleMap<xr::ActionSet, ActionSetWrapper> {
    unsafe {
        ACTION_SETS.as_ref().unwrap()
    }
}

pub fn actions() -> &'static HandleMap<xr::Action, ActionWrapper> {
    unsafe {
        ACTIONS.as_ref().unwrap()
    }
}

pub struct InstanceWrapper {
    pub handle: xr::Instance,
    pub sessions: RwLock<Vec<Arc<SessionWrapper>>>,
    pub action_sets: RwLock<Vec<Arc<ActionSetWrapper>>>,

    pub god_action_sets: HashMap<xr::Path, crate::god_actions::GodActionSet>,

    pub application_name: String,
    pub application_version: u32,
    pub engine_name: String,
    pub engine_version: u32,

    pub core: openxr::raw::Instance,

    pub get_instance_proc_addr_next: pfn::GetInstanceProcAddr,
}

// #[derive(Debug)]
pub struct SessionWrapper {
    pub handle: xr::Session,
    pub instance: Weak<InstanceWrapper>,

    pub god_states: HashMap<xr::Path/* interactionProfile */, HashMap<xr::Path /* binding */, Arc<RwLock<crate::god_actions::GodState>>>>,
    pub attached_actions: OnceCell<HashMap<xr::ActionSet, HashMap<xr::Action, Vec<(xr::Path, Vec<xr::Path>)>>>>,
    pub cached_action_states: OnceCell<HashMap<xr::Action, RwLock<SubactionCollection<ActionState>>>>,
}

#[derive(Debug)]
pub struct ActionSetWrapper {
    pub handle: xr::ActionSet,
    pub instance: Weak<InstanceWrapper>,
    pub actions: RwLock<Vec<Arc<ActionWrapper>>>,

    pub name: String,
    pub localized_name: String,
    pub priority: u32,
}

#[derive(Debug)]
pub struct ActionWrapper {
    pub handle: xr::Action,
    pub action_set: Weak<ActionSetWrapper>, 
    pub name: String,

    pub action_type: xr::ActionType,
    pub subaction_paths: Vec<xr::Path>,
    pub localized_name: String,

    pub bindings: RwLock<HashMap<xr::Path, Vec<xr::Path>>>,
}

impl std::fmt::Debug for InstanceWrapper {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f, 
            "Instance {{ handle: {:?}, application_name: {:?}, application_version: {:?}, engine_name: {:?}, engine_version: {:?} }}", 
            self.handle, self.application_name, self.application_version, self.engine_name, self.engine_version
        )
    }
}

impl InstanceWrapper {
    #[inline]
    pub fn create_session(
        &self,
        create_info: *const xr::SessionCreateInfo, 
        session: *mut xr::Session
    ) -> xr::Result {
        unsafe {
            (self.core.create_session)(self.handle, create_info, session)
        }
    }

    #[inline]
    pub fn create_action_set(
        &self,
        create_info: *const xr::ActionSetCreateInfo, 
        action_set: *mut xr::ActionSet
    ) -> xr::Result {
        unsafe {
            (self.core.create_action_set)(self.handle, create_info, action_set)
        }
    }

    #[inline]
    pub fn suggest_interaction_profile_bindings(
        &self,
        suggested_bindings: *const xr::InteractionProfileSuggestedBinding, 
    ) -> xr::Result {
        unsafe {
            (self.core.suggest_interaction_profile_bindings)(self.handle, suggested_bindings)
        }
    }

    #[inline]
    pub fn string_to_path(
        &self,
        path_string: &str,
    ) -> openxr::Result<xr::Path> {
        unsafe {
            let str = CString::new(path_string).unwrap();
            let mut path = xr::Path::NULL;
            let result = (self.core.string_to_path)(self.handle, str.as_ptr(), &mut path);
            if result.into_raw() < 0 {
                Err(result)
            } else {
                Ok(path)
            }
        }
    }

    #[inline]
    pub fn destroy_instance(
        &self
    ) -> xr::Result {
        unsafe {
            (self.core.destroy_instance)(self.handle)
        }
    }

    #[inline]
    pub fn destroy_session(
        &self,
        session: xr::Session
    ) -> xr::Result {
        unsafe {
            (self.core.destroy_session)(session)
        }
    }

    #[inline]
    pub fn destroy_action_set(
        &self,
        action_set: xr::ActionSet
    ) -> xr::Result {
        unsafe {
            (self.core.destroy_action_set)(action_set)
        }
    }

    #[inline]
    pub fn destroy_action(
        &self,
        action: xr::Action
    ) -> xr::Result {
        unsafe {
            (self.core.destroy_action)(action)
        }
    }

    pub fn path_to_string(
        &self, 
        path: xr::Path,
    ) -> Result<String, xr::Result> {
        unsafe {
            let mut string = String::new();

            let mut len = 0;
            let result = (self.core.path_to_string)(self.handle, path, 0, &mut len, std::ptr::null_mut());
            if result.into_raw() < 0 { return Err(result); }
            
            let mut buffer = Vec::<i8>::with_capacity(len as usize);
            buffer.set_len(len as usize);
    
            let result = (self.core.path_to_string)(self.handle, path, len, &mut len, buffer.as_mut_ptr());
            if result.into_raw() < 0 { return Err(result); }

            let slice = std::str::from_utf8(std::mem::transmute(&buffer[..len as usize - 1])).unwrap();
            string.clear();
            string.reserve(slice.len());
            string.insert_str(0, slice);

            Ok(string)
        }
    }

    pub fn from_handle_panic<'a>(handle: xr::Instance) -> HandleRef<'a, xr::Instance, InstanceWrapper> {
        unsafe {
            INSTANCES.as_ref().unwrap().get(&handle).unwrap()
        }
    }
}

impl SessionWrapper {
    pub fn new(handle: xr::Session, instance: &Arc<InstanceWrapper>) -> Result<Self> {
        let mut wrapper = Self {
            handle,
            instance: Arc::downgrade(instance),
            attached_actions: Default::default(),
            cached_action_states: Default::default(),
            god_states: Default::default(),
        };
    
        let god_action_sets = instance
            .god_action_sets
            .values()
            .map(|container| container.handle)
            .collect::<Vec<_>>();
    
        let attach_info = xr::SessionActionSetsAttachInfo {
            ty: xr::SessionActionSetsAttachInfo::TYPE,
            next: ptr::null(),
            count_action_sets: god_action_sets.len() as u32,
            action_sets: god_action_sets.as_ptr(),
        };
    
        let result = wrapper.attach_session_action_sets(&attach_info);
    
        if result.into_raw() < 0 {
            return Err(result);
        }
    
        for (profile_name, god_action_set) in &instance.god_action_sets {
            let states = match wrapper.god_states.get_mut(profile_name) {
                Some(states) => states,
                None => {
                    wrapper.god_states.insert(*profile_name, HashMap::new());
                    wrapper.god_states.get_mut(profile_name).unwrap()
                },
            };
    
            for god_action in god_action_set.god_actions.values() {
                if god_action.action_type.is_input() {
                    for subaction_path in &god_action.subaction_paths {
                        let name = instance.path_to_string(*subaction_path)?.add(&god_action.name);
                        println!("{}", &name);
    
                        states.insert(
                            instance.string_to_path(&name)?,
                            Arc::new(RwLock::new(crate::god_actions::GodState {
                                action_handle: god_action.handle,
                                name,
                                subaction_path: *subaction_path,
                                action_state: crate::god_actions::ActionState::new(god_action.action_type).unwrap(),
                            })),
                        );
                    }
                }
            }
    
            let bindings = states.iter().map(|(path, god_state)| {
                xr::ActionSuggestedBinding {
                    action: god_state.read().unwrap().action_handle,
                    binding: *path,
                }
            }).collect::<Vec<_>>();
    
            let suggested_bindings = xr::InteractionProfileSuggestedBinding {
                ty: xr::InteractionProfileSuggestedBinding::TYPE,
               next: ptr::null(),
                interaction_profile: *profile_name,
                count_suggested_bindings: bindings.len() as u32,
                suggested_bindings: bindings.as_ptr(),
            };
    
            instance.suggest_interaction_profile_bindings(&suggested_bindings);
        }
        
        Ok(wrapper)
    }

    #[inline]
    pub fn attach_session_action_sets(
        &self,
        attach_info: *const xr::SessionActionSetsAttachInfo,
    ) -> xr::Result {
        unsafe {
            (self.instance().core.attach_session_action_sets)(self.handle, attach_info)
        }
    }

    #[inline]
    pub fn instance(&self) -> Arc<InstanceWrapper> {
        self.instance.upgrade().unwrap().clone()
    }

    #[inline]
    pub fn sync_actions(
        &self,
        sync_info: *const xr::ActionsSyncInfo,
    ) -> xr::Result {
        unsafe {
            (self.instance().core.sync_actions)(self.handle, sync_info)
        }
    }

    #[inline]
    pub fn get_action_state_boolean(
        &self,
        get_info: *const xr::ActionStateGetInfo,
        state: *mut xr::ActionStateBoolean,
    ) -> xr::Result {
        unsafe {
            (self.instance().core.get_action_state_boolean)(self.handle, get_info, state)
        }
    }

    #[inline]
    pub fn get_action_state_float(
        &self,
        get_info: *const xr::ActionStateGetInfo,
        state: *mut xr::ActionStateFloat,
    ) -> xr::Result {
        unsafe {
            (self.instance().core.get_action_state_float)(self.handle, get_info, state)
        }
    }

    #[inline]
    pub fn get_action_state_vector2f(
        &self,
        get_info: *const xr::ActionStateGetInfo,
        state: *mut xr::ActionStateVector2f,
    ) -> xr::Result {
        unsafe {
            (self.instance().core.get_action_state_vector2f)(self.handle, get_info, state)
        }
    }

    #[inline]
    pub fn get_action_state_pose(
        &self,
        get_info: *const xr::ActionStateGetInfo,
        state: *mut xr::ActionStatePose,
    ) -> xr::Result {
        unsafe {
            (self.instance().core.get_action_state_pose)(self.handle, get_info, state)
        }
    }

    pub fn from_handle_panic<'a>(handle: xr::Session) -> HandleRef<'a, xr::Session, SessionWrapper>  {
        unsafe {
            SESSIONS.as_ref().unwrap().get(&handle).unwrap()
        }
    }
}

impl ActionSetWrapper {
    #[inline]
    pub fn create_action(
        &self,
        create_info: *const xr::ActionCreateInfo, 
        action: *mut xr::Action
    ) -> xr::Result {
        unsafe {
            (self.instance().core.create_action)(self.handle, create_info, action)
        }
    }

    #[inline]
    pub fn instance(&self) -> Arc<InstanceWrapper> {
        self.instance.upgrade().unwrap().clone()
    }

    pub fn from_handle_panic<'a>(handle: xr::ActionSet) -> HandleRef<'a, xr::ActionSet, ActionSetWrapper> {
        unsafe {
            ACTION_SETS.as_ref().unwrap().get(&handle).unwrap()
        }
    }
}

impl ActionWrapper {
    #[inline]
    pub fn action_set(&self) -> Arc<ActionSetWrapper> {
        self.action_set.upgrade().unwrap().clone()
    }

    pub fn from_handle_panic<'a>(handle: xr::Action) -> HandleRef<'a, xr::Action, ActionWrapper> {
        unsafe {
            ACTIONS.as_ref().unwrap().get(&handle).unwrap()
        }
    }
}

pub trait HandleWrapper {
    type HandleType: std::hash::Hash + core::cmp::Eq + 'static;

    fn all_handles() -> &'static HandleMap<Self::HandleType, Self> where Self: 'static;

    fn from_handle<'a>(handle: Self::HandleType) -> Option<HandleRef<'a, Self::HandleType, Self>> where Self: 'static {
        HandleWrapper::all_handles().get(&handle)
    }
}

impl HandleWrapper for InstanceWrapper {
    type HandleType = xr::Instance;

    fn all_handles() -> &'static HandleMap<Self::HandleType, Self> {
        instances()
    }
}

impl HandleWrapper for SessionWrapper {
    type HandleType = xr::Session;

    fn all_handles() -> &'static HandleMap<Self::HandleType, Self> {
        sessions()
    }
}

impl HandleWrapper for ActionSetWrapper {
    type HandleType = xr::ActionSet;

    fn all_handles() -> &'static HandleMap<Self::HandleType, Self> where Self: 'static {
        action_sets()
    }
}

impl HandleWrapper for ActionWrapper {
    type HandleType = xr::Action;

    fn all_handles() -> &'static HandleMap<Self::HandleType, Self> where Self: 'static {
        actions()
    }
}

pub trait WrappedHandle {
    type Wrapper: HandleWrapper<HandleType = Self>;

    fn get_wrapper<'a>(self) -> Option<HandleRef<'a, Self, Self::Wrapper>> where Self: Sized + 'static {
        Self::Wrapper::from_handle(self)
    }
}

impl WrappedHandle for xr::Instance {
    type Wrapper = InstanceWrapper;
}

impl WrappedHandle for xr::Session {
    type Wrapper = SessionWrapper;
}

impl WrappedHandle for xr::ActionSet {
    type Wrapper = ActionSetWrapper;
}

impl WrappedHandle for xr::Action {
    type Wrapper = ActionWrapper;
}