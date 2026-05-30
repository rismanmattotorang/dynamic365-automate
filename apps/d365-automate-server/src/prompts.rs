//! MCP prompts.
//!
//! Server-rendered prompt templates encapsulating Dynamics 365 workflows the
//! model would otherwise compose from scratch. Three built-ins plus every
//! skill auto-loaded from `./skills/*.md`.

use d365_automate_skills::{Skill, SkillRegistry};
use mcp_core::{GetPromptResult, Prompt, PromptArgument, PromptMessage, Role, ToolContent};
use mcp_server::{registry::PromptHandler, PromptDescriptor};
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

pub fn all(skill_registry: &SkillRegistry) -> Vec<PromptDescriptor> {
    let mut out = vec![
        review_service_call(),
        deploy_impact_analysis(),
        review_cross_reference(),
    ];
    for skill in skill_registry.skills() {
        out.push(skill_as_prompt(skill.clone()));
    }
    out
}

fn skill_as_prompt(skill: Skill) -> PromptDescriptor {
    let skill_for_handler = skill.clone();
    struct H(Skill);
    impl PromptHandler for H {
        fn get(
            &self,
            arguments: Option<serde_json::Value>,
        ) -> Pin<Box<dyn Future<Output = mcp_core::Result<GetPromptResult>> + Send + '_>> {
            let skill = self.0.clone();
            Box::pin(async move {
                let arg_map: HashMap<String, String> = match arguments {
                    Some(serde_json::Value::Object(m)) => m
                        .into_iter()
                        .filter_map(|(k, v)| v.as_str().map(|s| (k, s.to_string())))
                        .collect(),
                    _ => HashMap::new(),
                };
                let body = skill.render(&arg_map);
                Ok(GetPromptResult {
                    description: Some(skill.description.clone()),
                    messages: vec![PromptMessage {
                        role: Role::User,
                        content: ToolContent::text(body),
                    }],
                })
            })
        }
    }
    PromptDescriptor {
        prompt: Prompt {
            name: skill_for_handler.name.clone(),
            description: Some(skill_for_handler.description.clone()),
            arguments: skill_for_handler
                .arguments
                .iter()
                .map(|a| PromptArgument {
                    name: a.name.clone(),
                    description: a.description.clone(),
                    required: a.required,
                })
                .collect(),
        },
        handler: Arc::new(H(skill_for_handler)),
    }
}

fn review_cross_reference() -> PromptDescriptor {
    struct H;
    impl PromptHandler for H {
        fn get(
            &self,
            arguments: Option<serde_json::Value>,
        ) -> Pin<Box<dyn Future<Output = mcp_core::Result<GetPromptResult>> + Send + '_>> {
            let args = arguments.unwrap_or(serde_json::Value::Object(Default::default()));
            let object = args
                .get("object")
                .and_then(|v| v.as_str())
                .unwrap_or("<OBJECT>")
                .to_string();
            let kind = args
                .get("kind")
                .and_then(|v| v.as_str())
                .unwrap_or("class")
                .to_string();
            Box::pin(async move {
                let body = format!(
                    "Before changing or deploying {kind} {object}, run xpp.meta.cross_reference and reason carefully about the impact.\n\nSteps:\n1. Call xpp.meta.cross_reference with name={object}, kind={kind} to enumerate every caller / implementer / reference site.\n2. For each hit, group by model using xpp.meta.get_model_contents on the parent.\n3. Identify which callers sit on a hot path (cross-reference business processes via flow.find_process).\n4. Produce a 3-section report: Direct callers, Indirect dependents, Recommended pre-deploy checks (regression tests, Solution Checker, packages to coordinate).\n\nCite every claim by its source URI (d365-service://, d365-entity://, or d365-learn://)."
                );
                Ok(GetPromptResult {
                    description: Some(
                        "Cross-reference review before changing or deploying an X++ object.".into(),
                    ),
                    messages: vec![PromptMessage {
                        role: Role::User,
                        content: ToolContent::text(body),
                    }],
                })
            })
        }
    }
    PromptDescriptor {
        prompt: Prompt {
            name: "xpp.review-cross-reference".into(),
            description: Some(
                "Walk the agent through a cross-reference analysis before changing an X++ object."
                    .into(),
            ),
            arguments: vec![
                PromptArgument {
                    name: "object".into(),
                    description: Some("Object name".into()),
                    required: true,
                },
                PromptArgument {
                    name: "kind".into(),
                    description: Some("Object kind (class | interface | table | ...)".into()),
                    required: false,
                },
            ],
        },
        handler: Arc::new(H),
    }
}

fn review_service_call() -> PromptDescriptor {
    struct H;
    impl PromptHandler for H {
        fn get(
            &self,
            arguments: Option<serde_json::Value>,
        ) -> Pin<Box<dyn Future<Output = mcp_core::Result<GetPromptResult>> + Send + '_>> {
            let args = arguments.unwrap_or(serde_json::Value::Object(Default::default()));
            let operation = args
                .get("operation")
                .and_then(|v| v.as_str())
                .unwrap_or("<UNKNOWN>")
                .to_string();
            let parameters = args
                .get("parameters")
                .cloned()
                .unwrap_or(serde_json::Value::Object(Default::default()));
            Box::pin(async move {
                let body = format!(
                    "Review the following proposed Dynamics 365 OData operation before execution. Confirm it is the right operation for the user's intent, that every required parameter is present and well-typed, that the values are realistic for the target environment, and that the side-effects are acceptable. Cite the source for each claim.\n\nOperation: {operation}\nParameters:\n{}\n\nIf safe, summarise what the call will do, the affected entities, and the user-visible result. If unsafe, identify the specific risk and propose a safer alternative.",
                    serde_json::to_string_pretty(&parameters).unwrap_or_default(),
                );
                Ok(GetPromptResult {
                    description: Some(
                        "Pre-execution review of a proposed d365.service.call".into(),
                    ),
                    messages: vec![PromptMessage {
                        role: Role::User,
                        content: ToolContent::text(body),
                    }],
                })
            })
        }
    }
    PromptDescriptor {
        prompt: Prompt {
            name: "d365.review-service-call".into(),
            description: Some(
                "Pre-flight review of a proposed d365.service.call invocation.".into(),
            ),
            arguments: vec![
                PromptArgument {
                    name: "operation".into(),
                    description: Some("OData operation name".into()),
                    required: true,
                },
                PromptArgument {
                    name: "parameters".into(),
                    description: Some("Parameters object".into()),
                    required: false,
                },
            ],
        },
        handler: Arc::new(H),
    }
}

fn deploy_impact_analysis() -> PromptDescriptor {
    struct H;
    impl PromptHandler for H {
        fn get(
            &self,
            arguments: Option<serde_json::Value>,
        ) -> Pin<Box<dyn Future<Output = mcp_core::Result<GetPromptResult>> + Send + '_>> {
            let args = arguments.unwrap_or(serde_json::Value::Object(Default::default()));
            let package = args
                .get("package")
                .and_then(|v| v.as_str())
                .unwrap_or("<PACKAGE>")
                .to_string();
            let scope = args
                .get("scope")
                .and_then(|v| v.as_str())
                .unwrap_or("PRODUCTION")
                .to_string();
            Box::pin(async move {
                let body = format!(
                    "Analyse the impact of deploying package {package} to the {scope} environment.\n\nSteps:\n1. Use d365.docs.search to find related Microsoft Learn content for the objects in the package.\n2. Use xpp.meta.search to find the X++ objects the package modifies.\n3. Use xpp.meta.cross_reference on each modified object to enumerate downstream callers.\n4. Use app.search_solutions to enumerate dependent solutions/modules.\n5. Produce a 3-section report: Direct impact, Indirect impact, Recommended pre-deploy checks (Solution Checker, db sync, regression).\n\nCite every claim by its source URI."
                );
                Ok(GetPromptResult {
                    description: Some(
                        "Cross-domain impact analysis for a Dynamics 365 deployable package".into(),
                    ),
                    messages: vec![PromptMessage {
                        role: Role::User,
                        content: ToolContent::text(body),
                    }],
                })
            })
        }
    }
    PromptDescriptor {
        prompt: Prompt {
            name: "d365.deploy-impact-analysis".into(),
            description: Some(
                "Multi-tool cross-domain impact analysis for a Dynamics 365 package deployment."
                    .into(),
            ),
            arguments: vec![
                PromptArgument {
                    name: "package".into(),
                    description: Some("Deployable package name".into()),
                    required: true,
                },
                PromptArgument {
                    name: "scope".into(),
                    description: Some("Target environment (PRODUCTION / UAT / DEV)".into()),
                    required: false,
                },
            ],
        },
        handler: Arc::new(H),
    }
}
