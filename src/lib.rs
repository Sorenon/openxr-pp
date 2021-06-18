mod loader_interfaces;
mod wrappers;
mod serial;

use xr::Version;
use wrappers::*;
use loader_interfaces::*;

use openxr_sys as xr;
use openxr_sys::pfn as pfn;

use std::borrow::Borrow;
use std::cell::RefCell;
use std::collections::HashMap;
use std::os::raw::c_char;
use std::ffi::CStr;
use std::rc::Rc;

const LAYER_NAME: &'static str = "XR_APILAYER_BULLCH_openxr_pp";

static mut GET_INSTANCE_PROC_ADDR_NEXT: Option<pfn::GetInstanceProcAddr> = None;

#[no_mangle]
pub unsafe extern "system" fn xrNegotiateLoaderApiLayerInterface(
    loader_info: *const XrNegotiateLoaderInfo, 
    layer_name: *const i8,
    api_layer_request: *mut XrNegotiateApiLayerRequest
) -> xr::Result
{
    assert_eq!(LAYER_NAME, CStr::from_ptr(layer_name).to_str().unwrap());

    let loader_info = &*loader_info;
    let api_layer_request = &mut *api_layer_request;
    api_layer_request.layer_interface_version = loader_info.max_interface_version; 
    api_layer_request.layer_api_version = loader_info.max_api_version; 
    api_layer_request.get_instance_proc_addr = Some(instance_proc_addr);
    api_layer_request.create_api_layer_instance = Some(create_api_layer_instance);

    if INSTANCES.is_none() {
        INSTANCES = Some(HashMap::new());
        SESSIONS = Some(HashMap::new());
        ACTIONS = Some(HashMap::new());
        ACTION_SETS = Some(HashMap::new());
    }

    xr::Result::SUCCESS
}

unsafe extern "system" fn create_api_layer_instance(
    info: *const xr::InstanceCreateInfo, 
    layer_info: *const ApiLayerCreateInfo, 
    instance: *mut xr::Instance
) -> xr::Result 
{
    let next_info = &*(*layer_info).next_info;

    assert_eq!(LAYER_NAME, CStr::from_ptr(std::mem::transmute(next_info.layer_name.as_ptr())).to_str().unwrap());

    GET_INSTANCE_PROC_ADDR_NEXT = Some(next_info.next_get_instance_proc_addr.clone()); //Store the GetInstanceProcAddr func of the layer bellow us

    let result = { //Initialize the layer bellow us
        let mut my_create_info = (*layer_info).clone();
        my_create_info.next_info = next_info.next;

        (next_info.next_create_api_layer_instance)(info, std::ptr::addr_of!(my_create_info), instance)
    };

    if result.into_raw() < 0 { return result; }
    
    let application_info = &(*info).application_info;

    let wrapper = wrappers::Instance {
        handle: *instance,
        action_sets: Vec::new(),

        application_name:  i8_arr_to_owned(&application_info.application_name),
        application_version: application_info.application_version,
        engine_name: i8_arr_to_owned(&application_info.engine_name),
        engine_version: application_info.engine_version,

        create_session: std::mem::transmute(get_func(*instance, "xrCreateSession").unwrap()),
        create_action_set: std::mem::transmute(get_func(*instance, "xrCreateActionSet").unwrap()),
        create_action: std::mem::transmute(get_func(*instance, "xrCreateAction").unwrap()),
        attach_session_action_sets: std::mem::transmute(get_func(*instance, "xrAttachSessionActionSets").unwrap()),
        suggest_interaction_profile_bindings: std::mem::transmute(get_func(*instance, "xrSuggestInteractionProfileBindings").unwrap()),
        path_to_string: std::mem::transmute(get_func(*instance, "xrPathToString").unwrap()),
    };

    //Add this instance to the rt map
    INSTANCES.as_mut().unwrap().insert((*instance).into_raw(), Rc::new(RefCell::new(wrapper)));

    result
}

unsafe extern "system" fn instance_proc_addr(instance: xr::Instance, name: *const c_char, function: *mut Option<pfn::VoidFunction>) -> xr::Result {
    let result = GET_INSTANCE_PROC_ADDR_NEXT.unwrap()(instance, name, function);

    if result.into_raw() < 0 { return result; }

    let name_str = if let Ok(slice) = CStr::from_ptr(name).to_str() { slice } else { return xr::Result::ERROR_VALIDATION_FAILURE };
    println!("instance_proc_addr: {}", name_str);

    (*function) = Some(
        match name_str {
            "xrCreateSession" => std::mem::transmute(create_session as pfn::CreateSession),
            "xrCreateActionSet" => std::mem::transmute(create_action_set as pfn::CreateActionSet),
            "xrCreateAction" => std::mem::transmute(create_action as pfn::CreateAction),
            "xrSuggestInteractionProfileBindings" => std::mem::transmute(suggest_interaction_profile_bindings as pfn::SuggestInteractionProfileBindings),
            "xrAttachSessionActionSets" => std::mem::transmute(attach_session_action_sets as pfn::AttachSessionActionSets),
            _ => (*function).unwrap()
        }
    );

    result
}

unsafe extern "system" fn create_session(
    instance: xr::Instance,
    create_info: *const xr::SessionCreateInfo,
    session: *mut xr::Session,
) -> xr::Result {
    let instance = Instance::from_handle(instance);
    let result = instance.try_borrow().unwrap().create_session(create_info, session);

    if result.into_raw() < 0 { return result; }

    let meta = Rc::new(RefCell::new(Session {
        handle: *session,
        instance: Rc::downgrade(instance)
    }));

    //TODO Add this action_set to the rt tree

    //Add this action_set to the rt map
    SESSIONS.as_mut().unwrap().insert((*session).into_raw(), meta);

    result
}

unsafe extern "system" fn create_action_set(
    instance: xr::Instance, 
    create_info: *const xr::ActionSetCreateInfo, 
    action_set: *mut xr::ActionSet
) -> xr::Result {
    let instance = Instance::from_handle(instance);
    let result = instance.try_borrow().unwrap().create_action_set(create_info, action_set);

    if result.into_raw() < 0 { return result; }

    let create_info = *create_info;
    let name = i8_arr_to_owned(&create_info.action_set_name);
    let localized_name = i8_arr_to_owned(&create_info.localized_action_set_name);

    let meta = Rc::new(RefCell::new(ActionSet {
        handle: *action_set,
        instance: Rc::downgrade(instance),
        actions: Vec::new(),
        name: name.clone(),
        localized_name: localized_name.clone(),
        priority: create_info.priority
    }));

    //Add this action_set to the rt tree
    instance.try_borrow_mut().unwrap().action_sets.push(meta.clone());

    //Add this action_set to the rt map
    ACTION_SETS.as_mut().unwrap().insert((*action_set).into_raw(), meta);

    result
}

unsafe extern "system" fn create_action(
    action_set: xr::ActionSet, 
    create_info: *const xr::ActionCreateInfo, 
    action: *mut xr::Action
) -> xr::Result {
    let action_set = ActionSet::from_handle(action_set);

    let result = action_set.try_borrow().unwrap().create_action(create_info, action);
    
    if result.into_raw() < 0 { return result; }

    let create_info = *create_info;

    let meta = Rc::new(RefCell::new(Action {
        handle: *action,
        action_set: Rc::downgrade(action_set),
        name: i8_arr_to_owned(&create_info.action_name),
        action_type: create_info.action_type,
        subaction_paths: std::slice::from_raw_parts(create_info.subaction_paths, create_info.count_subaction_paths as usize).to_owned(),
        localized_name: i8_arr_to_owned(&create_info.localized_action_name)
    }));
    
    //Add this action to the rt tree
    action_set.try_borrow_mut().unwrap().actions.push(meta.clone());

    //Add this action to the rt map
    ACTIONS.as_mut().unwrap().insert((*action).into_raw(), meta);
    
    result
}

unsafe extern "system" fn attach_session_action_sets(
    session: xr::Session,
    attach_info: *const xr::SessionActionSetsAttachInfo,
) -> xr::Result {
    let session = Session::from_handle(session).try_borrow().unwrap();
    let result = session.attach_session_action_sets(attach_info);

    if result.into_raw() < 0 { return result; }

    let attach_info = *attach_info;
    let instance_rc = session.instance();
    let instance = instance_rc.try_borrow().unwrap();
    let mut path_string = String::new();

    let mut application_actions = serial::ApplicationActions {
        application_name: instance.application_name.clone(),
        action_sets: Vec::new()
    };

    //TODO tidy up
    let action_sets: Vec<serial::ActionSet> = {
        std::slice::from_raw_parts(attach_info.action_sets, attach_info.count_action_sets as usize)
        .iter().map(|action_set| -> serial::ActionSet {
            let action_set = ActionSet::from_handle(action_set.clone()).try_borrow().unwrap();
            serial::ActionSet {
                name: action_set.name.clone(),
                localized_name: action_set.localized_name.clone(),
                actions: action_set.actions.iter().map(|action| -> serial::Action {
                    let action = action.try_borrow().unwrap();
                    serial::Action {
                        name: action.name.clone(),
                        localized_name: action.localized_name.clone(),
                        action_type: match action.action_type {
                            xr::ActionType::BOOLEAN_INPUT => serial::ActionType::BooleanInput,
                            xr::ActionType::FLOAT_INPUT => serial::ActionType::FloatInput,
                            xr::ActionType::POSE_INPUT => serial::ActionType::PoseInput,
                            xr::ActionType::VECTOR2F_INPUT => serial::ActionType::Vector2fInput,
                            xr::ActionType::VIBRATION_OUTPUT => serial::ActionType::VibrationOutput,
                            _ => serial::ActionType::Unknown
                        },
                        subaction_paths: action.subaction_paths.iter().map(|path| -> String {
                            Instance::path_to_string(&instance, path.clone(), &mut path_string);
                            path_string.clone()
                        }).collect()
                    }
                }).collect()
            }    
        }).collect()
    };

    application_actions.action_sets = action_sets;
    
    println!("{}", serde_json::to_string_pretty(&application_actions).unwrap());
    // instance.application_actions.action_sets.

    result
}

unsafe extern "system" fn suggest_interaction_profile_bindings(
    instance: xr::Instance, 
    suggested_bindings_ptr: *const xr::InteractionProfileSuggestedBinding
) -> xr::Result {
    let instance = Instance::from_handle(instance).try_borrow().unwrap();
    
    let suggested_bindings = *suggested_bindings_ptr;

    let mut path_string = String::new();
    let result = instance.path_to_string(suggested_bindings.interaction_profile, &mut path_string);
    if result.into_raw() < 0 { return result; }

    println!("~~~~{}~~~~", path_string);

    let suggested_bindings_slice = std::slice::from_raw_parts(suggested_bindings.suggested_bindings, suggested_bindings.count_suggested_bindings as usize);
    for suggested_binding in suggested_bindings_slice {
        let result = instance.path_to_string(suggested_binding.binding, &mut path_string);
        if result.into_raw() < 0 { return result; }

        let action_meta = Action::from_handle(suggested_binding.action).try_borrow().unwrap();
        
        println!("=>{}, {}, {}", action_meta.action_set().try_borrow().unwrap().localized_name, action_meta.localized_name, path_string);
    }
    println!("~~~~~~");   

    instance.borrow().suggest_interaction_profile_bindings(suggested_bindings_ptr)
}

unsafe fn i8_arr_to_owned(arr: &[i8]) -> String {
    String::from(CStr::from_ptr(std::mem::transmute(arr.as_ptr())).to_str().unwrap())
}

unsafe fn get_func(instance: xr::Instance, name: &str) -> Option<pfn::VoidFunction> {
    let mut func: Option<pfn::VoidFunction> = None;
    
    if GET_INSTANCE_PROC_ADDR_NEXT.unwrap()(instance, format!("{}\0", name).as_ptr() as *const i8, std::ptr::addr_of_mut!(func)).into_raw() < 0 {
        return None;
    }

    func
}