use std::cell::{Cell, RefCell};
use std::rc::Rc;

use mlua::{Lua, LuaOptions, LuaSerdeExt, StdLib, Table, Value, Variadic};

use crate::model::{Environment, Header, RequestItem, ResponseData};

const MAX_INSTRUCTIONS: u32 = 5_000_000;

#[derive(Default)]
pub struct ScriptOutcome {
    pub logs: Vec<String>,
    pub error: Option<String>,
}

struct EnvHandle(Rc<RefCell<Environment>>);

impl mlua::UserData for EnvHandle {
    fn add_methods<'lua, M: mlua::UserDataMethods<'lua, Self>>(methods: &mut M) {
        methods.add_method("get", |_, this, key: String| {
            Ok(this
                .0
                .borrow()
                .variables
                .iter()
                .find(|(k, _)| *k == key)
                .map(|(_, v)| v.clone()))
        });
        methods.add_method("has", |_, this, key: String| {
            Ok(this.0.borrow().variables.iter().any(|(k, _)| *k == key))
        });
        methods.add_method("set", |_, this, (key, value): (String, String)| {
            let mut env = this.0.borrow_mut();
            match env.variables.iter_mut().find(|(k, _)| *k == key) {
                Some(entry) => entry.1 = value,
                None => env.variables.push((key, value)),
            }
            Ok(())
        });
        methods.add_method("unset", |_, this, key: String| {
            this.0.borrow_mut().variables.retain(|(k, _)| *k != key);
            Ok(())
        });
    }
}

fn new_sandboxed_lua() -> mlua::Result<Lua> {
    let lua = Lua::new_with(
        StdLib::TABLE | StdLib::STRING | StdLib::MATH,
        LuaOptions::default(),
    )?;
    install_instruction_limit(&lua);
    Ok(lua)
}

fn install_instruction_limit(lua: &Lua) {
    let instructions_run = Rc::new(Cell::new(0u32));
    lua.set_hook(
        mlua::HookTriggers::new().every_nth_instruction(10_000),
        move |_lua, _debug| {
            instructions_run.set(instructions_run.get() + 10_000);
            if instructions_run.get() > MAX_INSTRUCTIONS {
                Err(mlua::Error::RuntimeError(
                    "script exceeded the step limit (possible infinite loop)".into(),
                ))
            } else {
                Ok(())
            }
        },
    );
}

fn install_console<'lua>(lua: &'lua Lua, pm: &Table<'lua>, logs: Rc<RefCell<Vec<String>>>) -> mlua::Result<()> {
    let console = lua.create_table()?;
    let log_fn = lua.create_function(move |lua, args: Variadic<Value>| {
        let tostring: mlua::Function = lua.globals().get("tostring")?;
        let mut parts = Vec::with_capacity(args.len());
        for v in args.iter() {
            let s: String = tostring.call(v.clone())?;
            parts.push(s);
        }
        logs.borrow_mut().push(parts.join("\t"));
        Ok(())
    })?;
    console.set("log", log_fn)?;
    pm.set("console", console)?;
    Ok(())
}

fn install_environment<'lua>(pm: &Table<'lua>, env: Rc<RefCell<Environment>>) -> mlua::Result<()> {
    pm.set("environment", EnvHandle(env))?;
    Ok(())
}

fn install_pm_test<'lua>(lua: &'lua Lua, pm: &Table<'lua>, logs: Rc<RefCell<Vec<String>>>) -> mlua::Result<()> {
    let test_fn = lua.create_function(move |_, (name, f): (String, mlua::Function)| {
        match f.call::<_, ()>(()) {
            Ok(()) => logs.borrow_mut().push(format!("PASS {name}")),
            Err(e) => logs.borrow_mut().push(format!("FAIL {name}: {e}")),
        }
        Ok(())
    })?;
    pm.set("test", test_fn)?;
    Ok(())
}

fn build_request_table<'lua>(lua: &'lua Lua, req: &RequestItem) -> mlua::Result<Table<'lua>> {
    let request = lua.create_table()?;
    request.set("method", req.method.as_str())?;
    request.set("url", req.url.clone())?;
    request.set("body", req.body.clone())?;
    let headers = lua.create_table()?;
    for h in req.headers.iter().filter(|h| h.enabled) {
        headers.set(h.key.clone(), h.value.clone())?;
    }
    request.set("headers", headers)?;
    Ok(request)
}

fn apply_request_table_back(request: Table, req: &mut RequestItem) -> mlua::Result<()> {
    req.url = request.get("url")?;
    req.body = request.get("body")?;
    let headers: Table = request.get("headers")?;
    let mut new_headers = Vec::new();
    for pair in headers.pairs::<String, String>() {
        let (k, v) = pair?;
        new_headers.push(Header { key: k, value: v, enabled: true });
    }
    req.headers = new_headers;
    Ok(())
}

fn build_response_table<'lua>(lua: &'lua Lua, resp: &ResponseData) -> mlua::Result<Table<'lua>> {
    let response = lua.create_table()?;
    response.set("code", resp.status)?;
    response.set("body", resp.body.clone())?;
    let headers = lua.create_table()?;
    for (k, v) in &resp.headers {
        headers.set(k.clone(), v.clone())?;
    }
    response.set("headers", headers)?;

    let body_for_json = resp.body.clone();
    let json_fn = lua.create_function(move |lua, ()| {
        let value: serde_json::Value = serde_json::from_str(&body_for_json)
            .map_err(|e| mlua::Error::RuntimeError(format!("response body is not valid JSON: {e}")))?;
        lua.to_value(&value)
    })?;
    response.set("json", json_fn)?;

    Ok(response)
}

fn take_logs(logs: Rc<RefCell<Vec<String>>>) -> Vec<String> {
    Rc::try_unwrap(logs).map(RefCell::into_inner).unwrap_or_default()
}

fn finish(logs: Rc<RefCell<Vec<String>>>, result: mlua::Result<()>) -> ScriptOutcome {
    let logs = take_logs(logs);
    match result {
        Ok(()) => ScriptOutcome { logs, error: None },
        Err(e) => ScriptOutcome { logs, error: Some(format!("{e}")) },
    }
}

pub fn run_pre_request(script: &str, req: &mut RequestItem, env: &mut Option<Environment>) -> ScriptOutcome {
    if script.trim().is_empty() {
        return ScriptOutcome::default();
    }

    let logs = Rc::new(RefCell::new(Vec::new()));
    let env_handle = env.clone().map(|e| Rc::new(RefCell::new(e)));

    let result = (|| -> mlua::Result<()> {
        let lua = new_sandboxed_lua()?;
        let pm = lua.create_table()?;

        install_console(&lua, &pm, logs.clone())?;
        if let Some(handle) = &env_handle {
            install_environment(&pm, handle.clone())?;
        }
        pm.set("request", build_request_table(&lua, req)?)?;

        lua.globals().set("pm", pm.clone())?;
        lua.load(script).exec()?;

        apply_request_table_back(pm.get("request")?, req)
    })();

    if let (Some(handle), Ok(())) = (&env_handle, &result) {
        *env = Some(handle.borrow().clone());
    }

    finish(logs, result)
}

pub fn run_post_response(script: &str, resp: &ResponseData, env: &mut Option<Environment>) -> ScriptOutcome {
    if script.trim().is_empty() {
        return ScriptOutcome::default();
    }

    let logs = Rc::new(RefCell::new(Vec::new()));
    let env_handle = env.clone().map(|e| Rc::new(RefCell::new(e)));

    let result = (|| -> mlua::Result<()> {
        let lua = new_sandboxed_lua()?;
        let pm = lua.create_table()?;

        install_console(&lua, &pm, logs.clone())?;
        if let Some(handle) = &env_handle {
            install_environment(&pm, handle.clone())?;
        }
        install_pm_test(&lua, &pm, logs.clone())?;
        pm.set("response", build_response_table(&lua, resp)?)?;

        lua.globals().set("pm", pm)?;
        lua.load(script).exec()
    })();

    if let (Some(handle), Ok(())) = (&env_handle, &result) {
        *env = Some(handle.borrow().clone());
    }

    finish(logs, result)
}
