use serde_json::Value;

/// Reject an inbound server message that uses an undeclared feature.
pub fn check(capabilities: &Value, message: &Value) -> Result<(), String> {
    let method = message["method"].as_str().unwrap_or("");
    if method.is_empty() && !message["result"]["effects"].is_array() { return Ok(()); }
    let params = &message["params"];
    let rich = &capabilities["experimental"]["magit"];
    let require = |declared: bool, feature: &str| {
        if declared { Ok(()) } else { Err(format!("undeclared server feature: {feature} ({method})")) }
    };
    match method {
        "workspace/applyEdit" => {
            require(capabilities["workspace"]["applyEdit"] == true, "workspace.applyEdit")?;
            if params["edit"]["documentChanges"].is_array() {
                require(capabilities["workspace"]["workspaceEdit"]["documentChanges"] == true, "workspaceEdit.documentChanges")?;
            }
        }
        "window/showDocument" => require(capabilities["window"]["showDocument"]["support"] == true, "showDocument")?,
        "window/workDoneProgress/create" | "$/progress" => require(capabilities["window"]["workDoneProgress"] == true, "workDoneProgress")?,
        "view/render" | "view/keymap" => require(rich.is_object(), "rich rendering")?,
        "view/prompt" => {
            let kind = params["kind"].as_str().unwrap_or("");
            let prompt = if params["secret"] == true { "secret" } else { kind };
            require(rich["prompts"].as_array().is_some_and(|items| items.iter().any(|item| item == prompt)), "prompt")?;
        }
        "view/effects" => {
            require(rich.is_object(), "rich effects")?;
            check_effects(rich, &params["effects"])?;
        }
        "$/logTrace" | "$/setTrace" | "window/logMessage" | "window/showMessage" => {}
        other => return Err(format!("unknown server method: {other}")),
    }
    if message["result"]["effects"].is_array() {
        check_effects(rich, &message["result"]["effects"])?;
    }
    if let Some(placement) = params["descriptor"]["placement"].as_str() {
        require(rich["layout"].as_array().is_some_and(|items| items.iter().any(|item| item == placement)), "layout placement")?;
    }
    Ok(())
}

fn check_effects(rich: &Value, effects: &Value) -> Result<(), String> {
    let core = ["message", "passThrough", "pushView", "popView", "focusView", "quit"];
    for effect in effects.as_array().into_iter().flatten() {
        let name = effect["type"].as_str().unwrap_or("");
        if core.contains(&name) {
            if name == "pushView" {
                if let Some(placement) = effect["descriptor"]["placement"].as_str() {
                    if !["main", "side", "bottom", "float"].contains(&placement) || !rich["layout"].as_array().is_some_and(|items| items.iter().any(|item| item == placement)) {
                        return Err(format!("undeclared server feature: layout placement {placement}"));
                    }
                }
            }
            continue;
        }
        let required = match name {
            "clipboard" => "clipboard",
            "visitFile" => "visitFile",
            "editDocument" => "editDocument",
            _ => return Err(format!("unknown server effect: {name}")),
        };
        if !rich["effects"].as_array().is_some_and(|items| items.iter().any(|item| item == required)) {
            return Err(format!("undeclared server feature: optional effect {required}"));
        }
    }
    Ok(())
}
