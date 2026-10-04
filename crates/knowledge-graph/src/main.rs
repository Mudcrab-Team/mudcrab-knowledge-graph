use clap::{Parser, Subcommand};
use mudcrab_knowledge_graph::{
    bundle,
    error::{Error, Result},
    model::*,
    query::*,
    source,
    store::{ContextManifest, Store},
};
use serde_json::{Value, json};
use std::path::PathBuf;
#[derive(Parser)]
#[command(
    name = "mudcrab-kg",
    about = "Local, versioned research evidence graph"
)]
struct Cli {
    #[arg(long, global = true, default_value = ".mudcrab-kg")]
    root: PathBuf,
    #[arg(long, global = true)]
    context: Option<String>,
    #[arg(long, global = true)]
    json: bool,
    #[command(subcommand)]
    command: Commands,
}
#[derive(Subcommand)]
enum Commands {
    Init,
    #[cfg(feature = "mcp")]
    Serve {
        #[arg(long, default_value = "stdio")]
        transport: String,
        #[arg(long, default_value = "public")]
        policy: String,
        #[arg(long)]
        acknowledge_client_export: bool,
    },
    Proposals {
        #[command(subcommand)]
        kind: ProposalCommand,
    },
    Export {
        #[arg(long, default_value = "public")]
        policy: String,
        #[arg(long)]
        out: PathBuf,
    },
    Conflict {
        #[arg(long)]
        file: PathBuf,
    },
    Doctor {
        #[arg(long)]
        adapter: Option<String>,
    },
    Ingest {
        #[command(subcommand)]
        kind: Ingest,
    },
    Context {
        #[command(subcommand)]
        kind: ContextCommand,
    },
    Search {
        #[arg(long)]
        query: String,
        #[arg(long, default_value_t = 20)]
        limit: u32,
        #[arg(long)]
        kind: Vec<NodeKind>,
        #[arg(long)]
        source: Vec<String>,
        #[arg(long)]
        cursor: Option<String>,
    },
    Node {
        #[arg(long)]
        id: String,
    },
    Neighbors {
        #[arg(long)]
        id: String,
        #[arg(long, default_value = "out")]
        direction: String,
        #[arg(long)]
        predicate: Vec<Predicate>,
        #[arg(long, default_value_t = 20)]
        limit: u32,
        #[arg(long)]
        cursor: Option<String>,
    },
    Trace {
        #[command(subcommand)]
        kind: Trace,
    },
    Evidence {
        #[arg(long)]
        id: String,
        #[arg(long, default_value_t = 8192)]
        max_bytes: usize,
    },
    Coverage {
        #[arg(long)]
        source: Vec<String>,
        #[arg(long)]
        capability: Vec<String>,
    },
    Review {
        #[arg(long)]
        assertion: String,
        #[arg(long)]
        actor: String,
        #[arg(long)]
        decision: String,
        #[arg(long)]
        reason: String,
    },
}
#[derive(Subcommand)]
enum ProposalCommand {
    Import {
        #[arg(long)]
        file: PathBuf,
    },
}
#[derive(Subcommand)]
enum Ingest {
    Observations {
        #[arg(long)]
        file: PathBuf,
        #[arg(long)]
        source: String,
    },
    Bundle {
        #[arg(long)]
        bundle: PathBuf,
        #[arg(long)]
        artifact_root: Option<PathBuf>,
    },
    Source {
        #[arg(long)]
        repo: PathBuf,
        #[arg(long)]
        source: String,
        #[arg(long)]
        revision: String,
        #[arg(long)]
        scip: Option<PathBuf>,
        #[arg(long, default_value = "source_structure")]
        mode: String,
        #[arg(long)]
        encoding_override: Option<String>,
        #[arg(long)]
        path: Vec<String>,
        #[arg(long)]
        origin: Option<String>,
        #[arg(long, default_value = "MIT OR Apache-2.0")]
        license_expression: String,
        #[arg(long, default_value = "with_notice")]
        redistribution: String,
        #[arg(long)]
        configuration: Option<PathBuf>,
    },
    Materials {
        #[arg(long)]
        dir: PathBuf,
        #[arg(long)]
        source: String,
        #[arg(long)]
        source_snapshot: Option<String>,
    },
    Registry {
        #[arg(long)]
        file: PathBuf,
    },
}
#[derive(Subcommand)]
enum ContextCommand {
    Create {
        #[arg(long)]
        name: String,
        #[arg(long)]
        manifest: PathBuf,
    },
    Pack {
        #[arg(long)]
        seed: Vec<String>,
        #[arg(long, default_value_t = 65536)]
        max_bytes: usize,
        #[arg(long)]
        out: Option<PathBuf>,
    },
}
#[derive(Subcommand)]
enum Trace {
    Material {
        #[arg(long)]
        asset: String,
        #[arg(long)]
        shape: Option<u32>,
        #[arg(long)]
        include_hypotheses: bool,
        #[arg(long, default_value_t = 20)]
        limit: u32,
    },
    Concept {
        #[arg(long)]
        concept: String,
        #[arg(long)]
        include_hypotheses: bool,
        #[arg(long, default_value_t = 20)]
        limit: u32,
    },
}
fn main() {
    let cli = Cli::parse();
    if let Err(e) = run(cli) {
        eprintln!(
            "{}",
            serde_json::to_string(&e).unwrap_or_else(|_| "{\"code\":\"STORE\"}".into())
        );
        std::process::exit(e.exit_code());
    }
}
fn run(cli: Cli) -> Result<()> {
    let result = match cli.command {
        #[cfg(feature = "mcp")]
        Commands::Serve {
            transport,
            policy,
            acknowledge_client_export,
        } => {
            if transport != "stdio" {
                return Err(Error::new("INVALID_INPUT", "MCP transport must be stdio"));
            }
            let ctx = cli
                .context
                .ok_or_else(|| Error::new("INVALID_INPUT", "serve requires --context"))?;
            return mudcrab_knowledge_graph::mcp::serve(
                &cli.root,
                &ctx,
                &policy,
                acknowledge_client_export,
            );
        }
        Commands::Export { policy, out } => {
            if policy != "public" {
                return Err(Error::new("POLICY", "export supports public policy only"));
            }
            let s = Store::open(&cli.root, false)?;
            let ctx = s.context(
                &cli.context
                    .ok_or_else(|| Error::new("INVALID_INPUT", "export requires context"))?,
                Some("public"),
            )?;
            mudcrab_knowledge_graph::export::public(&s, &ctx, &out)?
        }
        Commands::Proposals {
            kind: ProposalCommand::Import { file },
        } => {
            let mut s = Store::open(&cli.root, true)?;
            let raw = std::fs::read(file)?;
            let b = mudcrab_knowledge_graph::proposals::index(&s, &raw)?;
            s.ingest_bytes(&b.bundle, &b.bytes)?
        }
        Commands::Conflict { file } => {
            let mut s = Store::open(&cli.root, true)?;
            let ctx = s.context(
                &cli.context
                    .ok_or_else(|| Error::new("INVALID_INPUT", "conflict requires context"))?,
                None,
            )?;
            mudcrab_knowledge_graph::proposals::conflict(
                &mut s,
                ctx,
                serde_json::from_value(read_json(&file)?)?,
            )?
        }
        Commands::Init => {
            Store::init(&cli.root)?;
            json!({"schema_version":1,"status":"initialized"})
        }
        Commands::Doctor { adapter } => doctor(adapter.as_deref())?,
        Commands::Ingest { kind } => {
            let mut store = Store::open(&cli.root, true)?;
            match kind {
                Ingest::Observations { file, source } => {
                    let raw = std::fs::read(file)?;
                    let b = mudcrab_knowledge_graph::observations::index(&raw, &source)?;
                    store.ingest_bytes(&b.bundle, &b.bytes)?
                }
                Ingest::Bundle {
                    bundle: path,
                    artifact_root,
                } => {
                    let loaded = bundle::load(&path)?;
                    if loaded.diagnostics.iter().any(|d| d["severity"] == "error") {
                        return Err(Error::new(
                            "EXTRACTION_FAILED",
                            "producer reported extraction errors; import not published",
                        ));
                    }
                    let parent = if path.is_dir() {
                        path.clone()
                    } else {
                        path.parent()
                            .unwrap_or(std::path::Path::new("."))
                            .to_owned()
                    };
                    let default = parent.join("artifacts");
                    let root = artifact_root.as_deref().unwrap_or(&default);
                    store.ingest(&loaded.bundle, Some(root))?
                }
                Ingest::Source {
                    repo,
                    source: id,
                    revision,
                    scip,
                    mode,
                    encoding_override,
                    path,
                    origin,
                    license_expression,
                    redistribution,
                    configuration,
                } => {
                    if mode != "source_structure" && mode != "scip" {
                        return Err(Error::new(
                            "INVALID_INPUT",
                            "mode must be source_structure or scip",
                        ));
                    }
                    if mode == "scip" && scip.is_none() {
                        return Err(Error::new(
                            "MISSING_INPUT",
                            "SCIP mode needs a pre-generated index",
                        ));
                    }
                    let source = Source {
                        id: id.clone(),
                        kind: "repository".into(),
                        origin_uri: origin.unwrap_or_else(|| format!("local-source:{id}")),
                        visibility: Visibility::Public,
                        license_expression: Some(license_expression),
                        lineage_roots: vec![id],
                        redistribution,
                    };
                    let config=configuration.map(|p|read_json(&p)).transpose()?.unwrap_or_else(||json!({"target":"not_evaluated","features":"syntactic_all_conditions","build_scripts":"not_run","proc_macros":"not_run"}));
                    let builder = source::index(
                        &store,
                        source::SourceRequest {
                            repo: &repo,
                            source: &source,
                            revision: &revision,
                            selected: &path,
                            scip: scip.as_deref(),
                            encoding_override: encoding_override.as_deref(),
                            configuration: config,
                        },
                    )?;
                    store.ingest_bytes(&builder.bundle, &builder.bytes)?
                }
                Ingest::Materials {
                    dir,
                    source,
                    source_snapshot,
                } => {
                    let b = mudcrab_knowledge_graph::material::index(
                        &store,
                        &dir,
                        &source,
                        source_snapshot.as_deref(),
                    )?;
                    store.ingest_bytes(&b.bundle, &b.bytes)?
                }
                Ingest::Registry { file } => {
                    let config: source::Registry = toml::from_str(&std::fs::read_to_string(file)?)
                        .map_err(|_| {
                            Error::new("INVALID_INPUT", "invalid local source registry")
                        })?;
                    if config.schema_version != 1
                        || config.network_access
                        || config.execute_build_scripts
                        || config.execute_proc_macros
                    {
                        return Err(Error::new(
                            "POLICY",
                            "registry supports local read-only inputs only",
                        ));
                    }
                    let roots = read_json(&store.root.join("local-roots.json"))?;
                    let mut results = vec![];
                    for entry in config.sources {
                        if entry.origin == "UNRESOLVED"
                            || entry.revision == "UNRESOLVED"
                            || entry.lineage_roots.iter().any(|s| s == "UNRESOLVED")
                        {
                            return Err(Error::new(
                                "INVALID_INPUT",
                                "registry requires exact source origin/revision/lineage",
                            ));
                        }
                        if entry.kind != "repository" || entry.mode != "source_structure" {
                            return Err(Error::new(
                                "INVALID_INPUT",
                                "registry mode requires source_structure; import pre-generated SCIP explicitly",
                            ));
                        }
                        let path = roots[&entry.root_alias].as_str().ok_or_else(|| {
                            Error::new("MISSING_INPUT", "local source root alias unavailable")
                        })?;
                        let source = Source {
                            id: entry.id,
                            kind: entry.kind,
                            origin_uri: entry.origin,
                            visibility: entry.visibility,
                            license_expression: entry.license_expression,
                            lineage_roots: entry.lineage_roots,
                            redistribution: entry.redistribution,
                        };
                        let b = source::index(
                            &store,
                            source::SourceRequest {
                                repo: std::path::Path::new(path),
                                source: &source,
                                revision: &entry.revision,
                                selected: &entry.paths,
                                scip: None,
                                encoding_override: None,
                                configuration: json!({"mode":"opt_in_local_community","unexpanded_conditions":true}),
                            },
                        )?;
                        results.push(store.ingest_bytes(&b.bundle, &b.bytes)?);
                    }
                    json!({"imports":results})
                }
            }
        }
        Commands::Context {
            kind: ContextCommand::Create { name, manifest },
        } => {
            let mut m: ContextManifest = serde_json::from_value(read_json(&manifest)?)?;
            m.name = name;
            let mut store = Store::open(&cli.root, true)?;
            let ctx = store.create_context(&m)?;
            json!({"context_id":ctx.id,"snapshot_ids":ctx.snapshots,"policy":ctx.config})
        }
        Commands::Review {
            assertion,
            actor,
            decision,
            reason,
        } => Store::open(&cli.root, true)?.review(&assertion, &actor, &decision, &reason)?,
        command => {
            let store = Store::open(&cli.root, false)?;
            let name = cli
                .context
                .ok_or_else(|| Error::new("INVALID_INPUT", "query requires --context"))?;
            let q = Query::new(&store, store.context(&name, None)?);
            let envelope = match command {
                Commands::Search {
                    query,
                    limit,
                    kind,
                    source,
                    cursor,
                } => q.search(SearchInput {
                    query,
                    limit,
                    kinds: kind,
                    sources: source,
                    cursor,
                })?,
                Commands::Node { id } => q.node(NodeInput { node_id: id })?,
                Commands::Neighbors {
                    id,
                    direction,
                    predicate,
                    limit,
                    cursor,
                } => q.neighbors(NeighborsInput {
                    node_id: id,
                    direction,
                    predicates: predicate,
                    limit,
                    cursor,
                })?,
                Commands::Trace { kind } => match kind {
                    Trace::Material {
                        asset,
                        shape,
                        include_hypotheses,
                        limit,
                    } => {
                        let input = TraceInput {
                            mode: "material".into(),
                            seed: asset,
                            include_hypotheses,
                            limit,
                        };
                        if let Some(shape) = shape {
                            q.trace_material_shape(input, shape)?
                        } else {
                            q.trace(input)?
                        }
                    }
                    Trace::Concept {
                        concept,
                        include_hypotheses,
                        limit,
                    } => q.trace(TraceInput {
                        mode: "concept".into(),
                        seed: concept,
                        include_hypotheses,
                        limit,
                    })?,
                },
                Commands::Evidence { id, max_bytes } => q.evidence(EvidenceInput {
                    evidence_id: id,
                    max_bytes,
                })?,
                Commands::Coverage { source, capability } => q.coverage(CoverageInput {
                    sources: source,
                    capabilities: capability,
                })?,
                Commands::Context {
                    kind:
                        ContextCommand::Pack {
                            seed,
                            max_bytes,
                            out,
                        },
                } => {
                    let seeds = seed
                        .into_iter()
                        .map(|value| Seed {
                            kind: if value.starts_with("node_") {
                                "node"
                            } else {
                                "concept"
                            }
                            .into(),
                            value,
                        })
                        .collect();
                    let envelope = q.pack(ContextInput {
                        seeds,
                        max_bytes,
                        include_hypotheses: true,
                    })?;
                    if let Some(path) = out {
                        std::fs::write(path, serde_json::to_vec(&envelope)?)?;
                    }
                    envelope
                }
                _ => unreachable!("handled mutation"),
            };
            serde_json::to_value(envelope)?
        }
    };
    println!("{}", serde_json::to_string(&result)?);
    Ok(())
}
fn read_json(path: &std::path::Path) -> Result<Value> {
    let bytes = std::fs::read(path)?;
    if bytes.len() > 8 * 1024 * 1024 {
        return Err(Error::new(
            "INVALID_INPUT",
            "configuration exceeds record limit",
        ));
    }
    Ok(serde_json::from_slice(&bytes)?)
}
fn doctor(adapter: Option<&str>) -> Result<Value> {
    if adapter.is_some_and(|a| !matches!(a, "source" | "ghidra" | "renderdoc")) {
        return Err(Error::new("INVALID_INPUT", "unknown adapter"));
    }
    let ghidra = std::env::var_os("GHIDRA_INSTALL_DIR").is_some();
    let renderdoc = std::process::Command::new("python")
        .args([
            "-c",
            "import importlib.util; print(importlib.util.find_spec('renderdoc') is not None)",
        ])
        .output()
        .is_ok_and(|o| o.status.success() && o.stdout.starts_with(b"True"));
    Ok(
        json!({"source":{"status":"available","mode":"source_structure","build_execution":"disabled"},"ghidra":{"status":if ghidra{"configured_not_probed"}else{"unavailable"},"prerequisite":"existing analyzed program and PyGhidra installation"},"renderdoc":{"status":if renderdoc{"module_available_replay_not_probed"}else{"unavailable"},"prerequisite":"RenderDoc Python environment and supported existing capture"}}),
    )
}
