//! Offline mock Metadata client.
//!
//! Seeded with the GTFin / GTScm fixtures that mirror the knowledge-graph
//! seed (`d365-automate-graph`):
//!   - Jobs:        GTFinPostJournal, GTScmGrnCheck
//!   - Classes:     GTFinJournalPoster, GTFinConstants, GTFinValidation
//!   - Interfaces:  GTFinPostable
//!   - Data entities: LedgerJournalLineEntity
//!   - Tables:      LedgerJournalTrans
//!   - Models:      GTFin, GTScm
//!   - Cross-reference data wired between the above so impact analysis is
//!     meaningful in demos.

use crate::client::{MetaCallContext, MetadataClient};
use crate::connection::D365Connection;
use crate::error::{MetaError, MetaResult};
use crate::types::{
    CrossReferenceHit, CrossReferenceRequest, DataEntityView, DeployOutcome, DeployRequest,
    EntityRow, MetaSearchHit, MetaSearchRequest, ModelContents, ModelMember, ObjectSource,
    XppObjectKind, MAX_ENTITY_ROWS,
};
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;

pub struct MockMetadataClient {
    connection: D365Connection,
    classes: HashMap<String, ObjectSource>,
    interfaces: HashMap<String, ObjectSource>,
    tables: HashMap<String, ObjectSource>,
    jobs: HashMap<String, ObjectSource>,
    forms: HashMap<String, ObjectSource>,
    data_entities: HashMap<String, DataEntityView>,
    models: HashMap<String, ModelContents>,
    cross_refs: HashMap<(String, XppObjectKind), Vec<CrossReferenceHit>>,
    entity_data: HashMap<String, Vec<EntityRow>>,
}

impl MockMetadataClient {
    pub fn new(connection: D365Connection) -> Arc<Self> {
        let mut s = Self {
            connection,
            classes: HashMap::new(),
            interfaces: HashMap::new(),
            tables: HashMap::new(),
            jobs: HashMap::new(),
            forms: HashMap::new(),
            data_entities: HashMap::new(),
            models: HashMap::new(),
            cross_refs: HashMap::new(),
            entity_data: HashMap::new(),
        };
        s.seed();
        Arc::new(s)
    }

    fn seed(&mut self) {
        // Jobs
        self.jobs.insert("GTFinPostJournal".into(), obj(
            "GTFinPostJournal", XppObjectKind::Job, "GTFin", "Post GL journals",
            "class GTFinPostJournal\n{\n    public static void main(Args _args)\n    {\n        GTFinJournalPoster poster = new GTFinJournalPoster();\n        poster.post(GTFinConstants::defaultJournalName());\n    }\n}\n",
        ));
        self.jobs.insert("GTScmGrnCheck".into(), obj(
            "GTScmGrnCheck", XppObjectKind::Job, "GTScm", "Product receipt reconciliation",
            "class GTScmGrnCheck\n{\n    public static void main(Args _args)\n    {\n        // reconcile product receipts against PO\n    }\n}\n",
        ));

        // Classes
        self.classes.insert("GTFinJournalPoster".into(), obj(
            "GTFinJournalPoster", XppObjectKind::Class, "GTFin", "GL posting helper",
            "class GTFinJournalPoster implements GTFinPostable\n{\n    public boolean validate(LedgerJournalTable _hdr) { return true; }\n    public Voucher post(LedgerJournalName _name)\n    {\n        // calls the LedgerGeneralJournalEntry OData service via $batch\n        return '';\n    }\n}\n",
        ));
        self.classes.insert("GTFinConstants".into(), obj(
            "GTFinConstants", XppObjectKind::Class, "GTFin", "Shared GTFin constants",
            "class GTFinConstants\n{\n    public static LedgerJournalName defaultJournalName() { return 'GenJrn'; }\n}\n",
        ));
        self.classes.insert("GTFinValidation".into(), obj(
            "GTFinValidation", XppObjectKind::Class, "GTFin", "GL posting validation",
            "class GTFinValidation\n{\n    public static boolean periodOpen(TransDate _d) { return true; }\n}\n",
        ));

        // Interfaces
        self.interfaces.insert("GTFinPostable".into(), obj(
            "GTFinPostable", XppObjectKind::Interface, "GTFin", "GL posting contract",
            "interface GTFinPostable\n{\n    boolean validate(LedgerJournalTable _hdr);\n    Voucher post(LedgerJournalName _name);\n}\n",
        ));

        // Tables
        self.tables.insert("LedgerJournalTrans".into(), obj(
            "LedgerJournalTrans", XppObjectKind::Table, "ApplicationSuite", "General journal lines",
            "// table LedgerJournalTrans\n// fields: JournalNum, LineNum, LedgerDimension, AmountCurDebit, AmountCurCredit, DataAreaId\n",
        ));

        // Forms
        self.forms.insert("GTFinJournalForm".into(), obj(
            "GTFinJournalForm", XppObjectKind::Form, "GTFin", "General journal entry form",
            "[Form]\npublic class GTFinJournalForm extends FormRun\n{\n    // data sources: LedgerJournalTable, LedgerJournalTrans\n}\n",
        ));

        // Data entities
        self.data_entities.insert("LedgerJournalLineEntity".into(), DataEntityView {
            name: "LedgerJournalLineEntity".into(),
            public_entity_name: "LedgerJournalLines".into(),
            properties: serde_json::json!({
                "PublicCollectionName": "LedgerJournalLines",
                "DataManagementEnabled": "Yes",
                "IsReadOnly": "No",
                "PrimaryKey": ["JournalBatchNumber", "LineNumber"]
            }),
            source: "[DataEntity]\npublic class LedgerJournalLineEntity extends common\n{\n    // maps to table LedgerJournalTrans\n}\n".into(),
            line_count: 5,
        });

        // Models
        self.models.insert(
            "GTFin".into(),
            ModelContents {
                model: "GTFin".into(),
                description: Some("GaussianTech Finance customisations".into()),
                members: vec![
                    ModelMember {
                        name: "GTFinPostJournal".into(),
                        kind: XppObjectKind::Job,
                        description: Some("Post GL journals".into()),
                    },
                    ModelMember {
                        name: "GTFinJournalPoster".into(),
                        kind: XppObjectKind::Class,
                        description: Some("GL posting helper".into()),
                    },
                    ModelMember {
                        name: "GTFinPostable".into(),
                        kind: XppObjectKind::Interface,
                        description: Some("GL posting contract".into()),
                    },
                    ModelMember {
                        name: "GTFinConstants".into(),
                        kind: XppObjectKind::Class,
                        description: Some("Shared constants".into()),
                    },
                    ModelMember {
                        name: "GTFinValidation".into(),
                        kind: XppObjectKind::Class,
                        description: Some("Posting validation".into()),
                    },
                ],
            },
        );
        self.models.insert(
            "GTScm".into(),
            ModelContents {
                model: "GTScm".into(),
                description: Some("GaussianTech Supply Chain customisations".into()),
                members: vec![ModelMember {
                    name: "GTScmGrnCheck".into(),
                    kind: XppObjectKind::Job,
                    description: Some("Product receipt reconciliation".into()),
                }],
            },
        );

        // Cross-reference (who uses what)
        self.cross_refs.insert(
            ("GTFinPostable".into(), XppObjectKind::Interface),
            vec![CrossReferenceHit {
                object: "GTFinJournalPoster".into(),
                kind: XppObjectKind::Class,
                location: "class declaration".into(),
                usage: "implements".into(),
            }],
        );
        self.cross_refs.insert(
            ("GTFinJournalPoster".into(), XppObjectKind::Class),
            vec![CrossReferenceHit {
                object: "GTFinPostJournal".into(),
                kind: XppObjectKind::Job,
                location: "main()".into(),
                usage: "call".into(),
            }],
        );
        self.cross_refs.insert(
            ("LedgerJournalTrans".into(), XppObjectKind::Table),
            vec![
                CrossReferenceHit {
                    object: "LedgerJournalLineEntity".into(),
                    kind: XppObjectKind::DataEntity,
                    location: "data source".into(),
                    usage: "read".into(),
                },
                CrossReferenceHit {
                    object: "GTFinJournalPoster".into(),
                    kind: XppObjectKind::Class,
                    location: "post()".into(),
                    usage: "write".into(),
                },
            ],
        );

        // Entity data (for get_entity_contents)
        self.entity_data.insert(
            "LedgerJournalLineEntity".into(),
            vec![EntityRow {
                values: serde_json::json!({
                    "JournalBatchNumber": "000123", "LineNumber": 1, "AccountDisplayValue": "110180"
                })
                .as_object()
                .unwrap()
                .clone(),
            }],
        );
    }

    fn search_all(&self) -> impl Iterator<Item = (&ObjectSource,)> {
        self.classes
            .values()
            .chain(self.interfaces.values())
            .chain(self.tables.values())
            .chain(self.jobs.values())
            .chain(self.forms.values())
            .map(|o| (o,))
    }
}

fn obj(name: &str, kind: XppObjectKind, model: &str, desc: &str, source: &str) -> ObjectSource {
    ObjectSource {
        name: name.into(),
        kind,
        model: Some(model.into()),
        description: Some(desc.into()),
        source: source.into(),
        deployed: true,
        line_count: source.lines().count(),
    }
}

#[async_trait]
impl MetadataClient for MockMetadataClient {
    fn connection(&self) -> &D365Connection {
        &self.connection
    }

    async fn get_class(&self, name: &str) -> MetaResult<ObjectSource> {
        self.classes
            .get(name)
            .cloned()
            .ok_or_else(|| MetaError::NotFound {
                kind: "Class".into(),
                name: name.into(),
            })
    }
    async fn get_interface(&self, name: &str) -> MetaResult<ObjectSource> {
        self.interfaces
            .get(name)
            .cloned()
            .ok_or_else(|| MetaError::NotFound {
                kind: "Interface".into(),
                name: name.into(),
            })
    }
    async fn get_table(&self, name: &str) -> MetaResult<ObjectSource> {
        self.tables
            .get(name)
            .cloned()
            .ok_or_else(|| MetaError::NotFound {
                kind: "Table".into(),
                name: name.into(),
            })
    }
    async fn get_job(&self, name: &str) -> MetaResult<ObjectSource> {
        self.jobs
            .get(name)
            .cloned()
            .ok_or_else(|| MetaError::NotFound {
                kind: "Job".into(),
                name: name.into(),
            })
    }
    async fn get_form(&self, name: &str) -> MetaResult<ObjectSource> {
        self.forms
            .get(name)
            .cloned()
            .ok_or_else(|| MetaError::NotFound {
                kind: "Form".into(),
                name: name.into(),
            })
    }
    async fn get_model_contents(&self, model: &str) -> MetaResult<ModelContents> {
        self.models
            .get(model)
            .cloned()
            .ok_or_else(|| MetaError::NotFound {
                kind: "Model".into(),
                name: model.into(),
            })
    }
    async fn get_data_entity(&self, name: &str) -> MetaResult<DataEntityView> {
        self.data_entities
            .get(name)
            .cloned()
            .ok_or_else(|| MetaError::NotFound {
                kind: "DataEntity".into(),
                name: name.into(),
            })
    }

    async fn search(&self, request: MetaSearchRequest) -> MetaResult<Vec<MetaSearchHit>> {
        let q = request.query.to_lowercase();
        let terms: Vec<&str> = q.split_whitespace().collect();
        let mut hits: Vec<MetaSearchHit> = self
            .search_all()
            .filter(|(o,)| request.kind.is_none_or(|k| k == o.kind))
            .filter_map(|(o,)| {
                let hay = format!(
                    "{} {}",
                    o.name.to_lowercase(),
                    o.description.as_deref().unwrap_or("").to_lowercase()
                );
                let score: usize = terms.iter().map(|t| hay.matches(t).count()).sum();
                if score == 0 {
                    None
                } else {
                    Some(MetaSearchHit {
                        name: o.name.clone(),
                        kind: o.kind,
                        description: o.description.clone(),
                        model: o.model.clone(),
                        score: score as f32,
                    })
                }
            })
            .collect();
        hits.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        hits.truncate(request.max_results.max(1));
        Ok(hits)
    }

    async fn cross_reference(
        &self,
        request: CrossReferenceRequest,
    ) -> MetaResult<Vec<CrossReferenceHit>> {
        Ok(self
            .cross_refs
            .get(&(request.name.clone(), request.kind))
            .cloned()
            .unwrap_or_default())
    }

    async fn get_entity_contents(
        &self,
        entity: &str,
        max_rows: usize,
    ) -> MetaResult<Vec<EntityRow>> {
        if max_rows > MAX_ENTITY_ROWS {
            return Err(MetaError::EntityDataBlocked(format!(
                "requested {max_rows} rows exceeds the metadata-path cap of {MAX_ENTITY_ROWS}; use d365.entity.read"
            )));
        }
        let mut rows =
            self.entity_data
                .get(entity)
                .cloned()
                .ok_or_else(|| MetaError::NotFound {
                    kind: "DataEntity".into(),
                    name: entity.into(),
                })?;
        rows.truncate(max_rows);
        Ok(rows)
    }

    async fn deploy(
        &self,
        request: DeployRequest,
        ctx: MetaCallContext,
    ) -> MetaResult<DeployOutcome> {
        if ctx.read_only {
            return Err(MetaError::PermissionDenied(format!(
                "deploy of {} '{}' requires write mode (--enable-writes)",
                request.kind.label(),
                request.name,
            )));
        }
        // Mock deploy always "succeeds" (build + sync) for known objects.
        let known = self.classes.contains_key(&request.name)
            || self.jobs.contains_key(&request.name)
            || self.interfaces.contains_key(&request.name)
            || self.tables.contains_key(&request.name);
        if !known {
            return Err(MetaError::NotFound {
                kind: request.kind.label().into(),
                name: request.name,
            });
        }
        Ok(DeployOutcome {
            name: request.name.clone(),
            kind: request.kind,
            deployed: true,
            messages: vec![format!(
                "{} '{}' built and synchronised (mock)",
                request.kind.label(),
                request.name
            )],
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn client() -> Arc<MockMetadataClient> {
        MockMetadataClient::new(D365Connection::mock("dev"))
    }

    #[tokio::test]
    async fn reads_class_source() {
        let c = client();
        let cls = c.get_class("GTFinJournalPoster").await.unwrap();
        assert_eq!(cls.kind, XppObjectKind::Class);
        assert!(cls.source.contains("implements GTFinPostable"));
    }

    #[tokio::test]
    async fn search_finds_posting_objects() {
        let c = client();
        let hits = c
            .search(MetaSearchRequest {
                query: "post".into(),
                kind: None,
                max_results: 10,
            })
            .await
            .unwrap();
        assert!(hits.iter().any(|h| h.name == "GTFinPostJournal"));
    }

    #[tokio::test]
    async fn cross_reference_finds_implementers() {
        let c = client();
        let refs = c
            .cross_reference(CrossReferenceRequest {
                name: "GTFinPostable".into(),
                kind: XppObjectKind::Interface,
            })
            .await
            .unwrap();
        assert!(refs
            .iter()
            .any(|r| r.object == "GTFinJournalPoster" && r.usage == "implements"));
    }

    #[tokio::test]
    async fn deploy_is_blocked_in_read_only_mode() {
        let c = client();
        let err = c
            .deploy(
                DeployRequest {
                    name: "GTFinJournalPoster".into(),
                    kind: XppObjectKind::Class,
                },
                MetaCallContext { read_only: true },
            )
            .await
            .unwrap_err();
        assert!(matches!(err, MetaError::PermissionDenied(_)), "got {err:?}");
    }

    #[tokio::test]
    async fn deploy_succeeds_in_write_mode() {
        let c = client();
        let outcome = c
            .deploy(
                DeployRequest {
                    name: "GTFinJournalPoster".into(),
                    kind: XppObjectKind::Class,
                },
                MetaCallContext { read_only: false },
            )
            .await
            .unwrap();
        assert!(outcome.deployed);
    }

    #[tokio::test]
    async fn entity_contents_over_cap_is_blocked() {
        let c = client();
        let err = c
            .get_entity_contents("LedgerJournalLineEntity", 5000)
            .await
            .unwrap_err();
        assert!(
            matches!(err, MetaError::EntityDataBlocked(_)),
            "got {err:?}"
        );
    }
}
