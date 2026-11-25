use serde_json::Value;

/// Reject an inbound server message that uses an undeclared feature.
pub fn check(capabilities: &Value, message: &Value) -> Result<(), String> {
    let method = message["method"].as_str().unwrap_or("");
    if method.is_empty() {
        let result = &message["result"];
        if result["effects"]
            .as_array()
            .is_some_and(|effects| !effects.is_empty())
        {
            let rich = &capabilities["experimental"]["magit"];
            if rich.is_object() {
                check_effects(rich, &result["effects"])?;
            } else {
                // Standard executeCommand returns the shared outcome envelope;
                // Plain consumes stack transitions server-side (view_execution.rs).
                for effect in result["effects"].as_array().unwrap() {
                    require(
                        matches!(
                            effect["type"].as_str(),
                            Some("message" | "pushView" | "popView" | "focusView" | "quit")
                        ),
                        "rich response effects",
                        method,
                    )?;
                }
            }
        }
        if let Some(placement) = result["placement"]
            .as_str()
            .or(result["descriptor"]["placement"].as_str())
        {
            check_placement(&capabilities["experimental"]["magit"], placement)?;
        }
        return Ok(());
    }
    let params = &message["params"];
    let rich = &capabilities["experimental"]["magit"];
    let require = |declared: bool, feature: &str| {
        if declared {
            Ok(())
        } else {
            Err(format!("undeclared server feature: {feature} ({method})"))
        }
    };
    match method {
        "workspace/applyEdit" => {
            require(
                capabilities["workspace"]["applyEdit"] == true,
                "workspace.applyEdit",
            )?;
            if params["edit"]["documentChanges"].is_array() {
                require(
                    capabilities["workspace"]["workspaceEdit"]["documentChanges"] == true,
                    "workspaceEdit.documentChanges",
                )?;
            }
        }
        "window/showDocument" => require(
            capabilities["window"]["showDocument"]["support"] == true,
            "showDocument",
        )?,
        "window/workDoneProgress/create" | "$/progress" => require(
            capabilities["window"]["workDoneProgress"] == true,
            "workDoneProgress",
        )?,
        "view/render" => require(rich.is_object(), "rich rendering")?,
        "view/keymap" => {
            require(rich.is_object(), "rich rendering")?;
            require(rich["keys"] == "dynamic", "dynamic keymap delivery")?;
        }
        "view/prompt" => {
            let kind = params["kind"].as_str().unwrap_or("");
            let prompts = rich["prompts"].as_array();
            let declares =
                |prompt: &str| prompts.is_some_and(|items| items.iter().any(|item| item == prompt));
            require(
                declares(kind) && (params["secret"] != true || declares("secret")),
                "prompt",
            )?;
        }
        "view/effects" => {
            require(rich.is_object(), "rich effects")?;
            check_effects(rich, &params["effects"])?;
        }
        "$/logTrace" | "$/setTrace" | "window/logMessage" | "window/showMessage" => {}
        other => return Err(format!("unknown server method: {other}")),
    }
    if let Some(placement) = params["descriptor"]["placement"].as_str() {
        check_placement(rich, placement)?;
    }
    Ok(())
}

fn require(declared: bool, feature: &str, method: &str) -> Result<(), String> {
    if declared {
        Ok(())
    } else {
        Err(format!("undeclared server feature: {feature} ({method})"))
    }
}

fn check_placement(rich: &Value, placement: &str) -> Result<(), String> {
    if !["main", "side", "bottom", "float"].contains(&placement)
        || !rich["layout"]
            .as_array()
            .is_some_and(|items| items.iter().any(|item| item == placement))
    {
        return Err(format!(
            "undeclared server feature: layout placement {placement}"
        ));
    }
    Ok(())
}

fn check_effects(rich: &Value, effects: &Value) -> Result<(), String> {
    let core = [
        "message",
        "passThrough",
        "pushView",
        "popView",
        "focusView",
        "quit",
    ];
    for effect in effects.as_array().into_iter().flatten() {
        let name = effect["type"].as_str().unwrap_or("");
        if core.contains(&name) {
            if name == "pushView" {
                if let Some(placement) = effect["descriptor"]["placement"].as_str() {
                    if !["main", "side", "bottom", "float"].contains(&placement)
                        || !rich["layout"]
                            .as_array()
                            .is_some_and(|items| items.iter().any(|item| item == placement))
                    {
                        return Err(format!(
                            "undeclared server feature: layout placement {placement}"
                        ));
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
        if !rich["effects"]
            .as_array()
            .is_some_and(|items| items.iter().any(|item| item == required))
        {
            return Err(format!(
                "undeclared server feature: optional effect {required}"
            ));
        }
    }
    Ok(())
}
