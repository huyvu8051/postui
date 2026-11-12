use std::cell::{Cell, RefCell};
use std::rc::Rc;

use mlua::{
    FromLuaMulti, Lua, LuaOptions, LuaSerdeExt, MultiValue, StdLib, Table, Value, Variadic,
};

use crate::model::{Environment, Header, RequestItem, ResponseData};

const MAX_INSTRUCTIONS: u32 = 5_000_000;

#[derive(Default)]
pub struct ScriptOutcome {
    pub logs: Vec<String>,
    pub error: Option<String>,
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

fn install_console<'lua>(
    lua: &'lua Lua,
    pm: &Table<'lua>,
    logs: Rc<RefCell<Vec<String>>>,
) -> mlua::Result<()> {
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

/// `pm.clipboard.set(text)` — the only clipboard access scripts get (write-only).
fn install_clipboard<'lua>(
    lua: &'lua Lua,
    pm: &Table<'lua>,
    logs: Rc<RefCell<Vec<String>>>,
) -> mlua::Result<()> {
    let clipboard = lua.create_table()?;
    let set_fn = lua.create_function(move |_, text: String| {
        let chars = text.chars().count();
        arboard::Clipboard::new()
            .and_then(|mut c| c.set_text(text))
            .map_err(|e| mlua::Error::RuntimeError(format!("clipboard unavailable: {e}")))?;
        logs.borrow_mut()
            .push(format!("clipboard: copied {chars} chars"));
        Ok(())
    })?;
    clipboard.set("set", set_fn)?;
    pm.set("clipboard", clipboard)?;
    Ok(())
}

/// Drop the implicit `self` of a `pm.environment:fn(...)` call, so the Postman-style
/// `pm.environment.fn(...)` and the Lua method-style call both work.
fn without_self(args: MultiValue) -> MultiValue {
    let mut values = args.into_vec();
    if matches!(values.first(), Some(Value::Table(_))) {
        values.remove(0);
    }
    MultiValue::from_vec(values)
}

fn install_environment<'lua>(
    lua: &'lua Lua,
    pm: &Table<'lua>,
    env: Rc<RefCell<Environment>>,
) -> mlua::Result<()> {
    let environment = lua.create_table()?;

    let e = env.clone();
    environment.set(
        "get",
        lua.create_function(move |lua, args: MultiValue| {
            let key = String::from_lua_multi(without_self(args), lua)?;
            Ok(e.borrow()
                .variables
                .iter()
                .find(|(k, _)| *k == key)
                .map(|(_, v)| v.clone()))
        })?,
    )?;

    let e = env.clone();
    environment.set(
        "has",
        lua.create_function(move |lua, args: MultiValue| {
            let key = String::from_lua_multi(without_self(args), lua)?;
            Ok(e.borrow().variables.iter().any(|(k, _)| *k == key))
        })?,
    )?;

    let e = env.clone();
    environment.set(
        "set",
        lua.create_function(move |lua, args: MultiValue| {
            let (key, value) = <(String, String)>::from_lua_multi(without_self(args), lua)?;
            let mut env = e.borrow_mut();
            match env.variables.iter_mut().find(|(k, _)| *k == key) {
                Some(entry) => entry.1 = value,
                None => env.variables.push((key, value)),
            }
            Ok(())
        })?,
    )?;

    environment.set(
        "unset",
        lua.create_function(move |lua, args: MultiValue| {
            let key = String::from_lua_multi(without_self(args), lua)?;
            env.borrow_mut().variables.retain(|(k, _)| *k != key);
            Ok(())
        })?,
    )?;

    pm.set("environment", environment)?;
    Ok(())
}

fn install_pm_test<'lua>(
    lua: &'lua Lua,
    pm: &Table<'lua>,
    logs: Rc<RefCell<Vec<String>>>,
) -> mlua::Result<()> {
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
        new_headers.push(Header {
            key: k,
            value: v,
            enabled: true,
        });
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
        let value: serde_json::Value = serde_json::from_str(&body_for_json).map_err(|e| {
            mlua::Error::RuntimeError(format!("response body is not valid JSON: {e}"))
        })?;
        lua.to_value(&value)
    })?;
    response.set("json", json_fn)?;

    Ok(response)
}

fn take_logs(logs: Rc<RefCell<Vec<String>>>) -> Vec<String> {
    Rc::try_unwrap(logs)
        .map(RefCell::into_inner)
        .unwrap_or_default()
}

fn finish(logs: Rc<RefCell<Vec<String>>>, result: mlua::Result<()>) -> ScriptOutcome {
    let logs = take_logs(logs);
    match result {
        Ok(()) => ScriptOutcome { logs, error: None },
        Err(e) => ScriptOutcome {
            logs,
            error: Some(format!("{e}")),
        },
    }
}

pub fn run_pre_request(
    script: &str,
    req: &mut RequestItem,
    env: &mut Option<Environment>,
) -> ScriptOutcome {
    if script.trim().is_empty() {
        return ScriptOutcome::default();
    }

    let logs = Rc::new(RefCell::new(Vec::new()));
    let env_handle = env.clone().map(|e| Rc::new(RefCell::new(e)));

    let result = (|| -> mlua::Result<()> {
        let lua = new_sandboxed_lua()?;
        let pm = lua.create_table()?;

        install_console(&lua, &pm, logs.clone())?;
        install_clipboard(&lua, &pm, logs.clone())?;
        if let Some(handle) = &env_handle {
            install_environment(&lua, &pm, handle.clone())?;
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

pub fn run_post_response(
    script: &str,
    resp: &ResponseData,
    env: &mut Option<Environment>,
) -> ScriptOutcome {
    if script.trim().is_empty() {
        return ScriptOutcome::default();
    }

    let logs = Rc::new(RefCell::new(Vec::new()));
    let env_handle = env.clone().map(|e| Rc::new(RefCell::new(e)));

    let result = (|| -> mlua::Result<()> {
        let lua = new_sandboxed_lua()?;
        let pm = lua.create_table()?;

        install_console(&lua, &pm, logs.clone())?;
        install_clipboard(&lua, &pm, logs.clone())?;
        if let Some(handle) = &env_handle {
            install_environment(&lua, &pm, handle.clone())?;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clipboard_set_is_exposed_to_pre_and_post_scripts() {
        let mut req = RequestItem::new("t");
        let mut env = None;
        let out = run_pre_request("pm.console.log(type(pm.clipboard.set))", &mut req, &mut env);
        assert_eq!(out.error, None);
        assert_eq!(out.logs, vec!["function"]);

        let resp = ResponseData {
            status: 200,
            headers: vec![],
            body: String::new(),
            duration_ms: 0,
            ..Default::default()
        };
        let out = run_post_response("pm.console.log(type(pm.clipboard.set))", &resp, &mut env);
        assert_eq!(out.error, None);
        assert_eq!(out.logs, vec!["function"]);
    }

    fn env() -> Option<Environment> {
        Some(Environment::new("e"))
    }

    fn var(env: &Option<Environment>, key: &str) -> Option<String> {
        env.as_ref()?
            .variables
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.clone())
    }

    #[test]
    fn environment_accepts_postman_dot_and_lua_colon_calls() {
        let resp = ResponseData {
            status: 201,
            headers: vec![],
            body: String::new(),
            duration_ms: 0,
            ..Default::default()
        };
        let mut env = env();
        let script = r#"
            pm.environment.set("dot", "1")
            pm.environment:set("colon", tostring(pm.response.code))
            pm.environment.set("num", 42)
            pm.console.log(pm.environment.get("dot"), pm.environment:get("colon"))
            pm.console.log(tostring(pm.environment.has("dot")), tostring(pm.environment:has("nope")))
            pm.environment.set("gone", "x")
            pm.environment:unset("gone")
        "#;
        let out = run_post_response(script, &resp, &mut env);
        assert_eq!(out.error, None);
        assert_eq!(var(&env, "dot").as_deref(), Some("1"));
        assert_eq!(var(&env, "colon").as_deref(), Some("201"));
        assert_eq!(var(&env, "num").as_deref(), Some("42"));
        assert_eq!(var(&env, "gone"), None);
        assert_eq!(out.logs, vec!["1\t201", "true\tfalse"]);
    }
}
