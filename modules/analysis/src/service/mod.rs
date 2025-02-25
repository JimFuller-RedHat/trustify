mod load;
mod query;
mod walk;

pub use query::*;
pub use walk::*;

pub mod render;
#[cfg(test)]
mod test;

use crate::{
    Error,
    config::AnalysisConfig,
    model::{
        AnalysisStatus, BaseSummary, GraphMap, Node, PackageGraph, graph, graph::Node::External,
    },
};
use fixedbitset::FixedBitSet;
use futures::{StreamExt, stream};
use opentelemetry::global;
use parking_lot::Mutex;
use petgraph::{
    Direction,
    graph::{Graph, NodeIndex},
    prelude::EdgeRef,
    visit::{IntoNodeIdentifiers, VisitMap, Visitable},
};
use sea_orm::{
    ColumnTrait, EntityOrSelect, EntityTrait, QueryFilter, QueryOrder, QuerySelect, RelationTrait,
    prelude::ConnectionTrait,
};
use sea_query::{JoinType, Order};
use std::{
    collections::{HashMap, HashSet},
    fmt::Debug,
    sync::Arc,
};
use tracing::instrument;
use trustify_common::{
    db::query::Value,
    model::{Paginated, PaginatedResults},
};
use trustify_entity::{
    relationship::Relationship,
    sbom,
    sbom_external_node::{self, DiscriminatorType, ExternalType},
    sbom_node_checksum, source_document,
};
use uuid::Uuid;

#[derive(Clone)]
pub struct AnalysisService {
    graph_cache: Arc<GraphMap>,
}

async fn resolve_external_sbom<C: ConnectionTrait>(
    external_node_ref: String,
    connection: &C,
) -> Option<(Uuid, String)> {
    // we first lookup in sbom_external_node
    let sbom_external_node = match sbom_external_node::Entity::find()
        .filter(sbom_external_node::Column::NodeId.eq(external_node_ref))
        .one(connection)
        .await
    {
        Ok(Some(entity)) => entity,
        _ => return None,
    };

    match sbom_external_node.external_type {
        ExternalType::SPDX => {
            // for spdx, sbom_external_node discriminator_type and discriminator_value are used
            // to lookup sbom_id via join to SourceDocument. The node_id is just the external_node_ref.
            if sbom_external_node
                .discriminator_value
                .as_ref()
                .map(|s| s.is_empty())
                .unwrap_or(true)
            {
                return None;
            }
            if let Some(DiscriminatorType::Sha256) = sbom_external_node.discriminator_type {
                if let Some(discriminator_value) = sbom_external_node.discriminator_value {
                    match sbom::Entity::find()
                        .join(JoinType::Join, sbom::Relation::SourceDocument.def())
                        .filter(source_document::Column::Sha256.eq(discriminator_value.to_string()))
                        .one(connection)
                        .await
                    {
                        Ok(Some(entity)) => {
                            return Some((entity.sbom_id, sbom_external_node.external_node_ref));
                        }
                        _ => return None,
                    }
                }
            }
            None
        }
        ExternalType::CycloneDx => {
            // for cyclonedx, sbom_external_node discriminator_type and discriminator_value are used
            // we construct external_doc_id to lookup sbom_id directly from sbom entity. The node_id
            // is the external_node_ref
            if sbom_external_node
                .discriminator_value
                .as_ref()
                .map(|s| s.is_empty())
                .unwrap_or(true)
            {
                return None;
            }
            if let Some(discriminator_value) = sbom_external_node.discriminator_value {
                let external_doc_ref = sbom_external_node.external_doc_ref;
                let external_doc_id =
                    format!("urn:cdx:{}/{}", external_doc_ref, discriminator_value);
                match sbom::Entity::find()
                    .filter(sbom::Column::DocumentId.eq(external_doc_id))
                    .one(connection)
                    .await
                {
                    Ok(Some(entity)) => {
                        return Some((entity.sbom_id, sbom_external_node.external_node_ref));
                    }
                    _ => return None,
                }
            }
            None
        }
        ExternalType::RedHatProductComponent => {
            // for RH variations we assume the sbom_external_node_ref is the package checksum
            // which is used on sbom_node_checksum to lookup related value then
            // perform another lookup on sbom_node_checksum (matching by value) to find resultant
            // sbom_id/node_id
            let sbom_external_node_ref = sbom_external_node.external_node_ref;

            match sbom_node_checksum::Entity::find()
                .filter(sbom_node_checksum::Column::NodeId.eq(sbom_external_node_ref.to_string()))
                .one(connection)
                .await
            {
                Ok(Some(entity)) => {
                    match sbom_node_checksum::Entity::find()
                        .filter(sbom_node_checksum::Column::SbomId.ne(entity.sbom_id))
                        .filter(sbom_node_checksum::Column::Value.eq(entity.value.to_string()))
                        .one(connection)
                        .await
                    {
                        Ok(Some(matched)) => Some((matched.sbom_id, matched.node_id)),
                        _ => None,
                    }
                }
                _ => None,
            }
        }
    }
}

/// Tracker for visited nodes, across graphs,
#[derive(Default, Clone)]
struct DiscoveredTracker {
    cache: Arc<
        Mutex<HashMap<*const Graph<graph::Node, Relationship, petgraph::Directed>, FixedBitSet>>,
    >,
}

impl DiscoveredTracker {
    pub fn visit(
        &self,
        graph: &Graph<graph::Node, Relationship, petgraph::Directed>,
        node: NodeIndex,
    ) -> bool {
        let mut maps = self.cache.lock();
        let map = maps
            .entry(graph as *const Graph<_, _> as *const _)
            .or_insert_with(|| graph.visit_map());

        map.visit(node)
    }
}

/// Collector, helping on collector nodes from a graph.
///
/// Keeping track of all relevant information.
struct Collector<'a, C: ConnectionTrait> {
    graphs: &'a [(String, Arc<PackageGraph>)],
    graph: &'a Graph<graph::Node, Relationship, petgraph::Directed>,
    node: NodeIndex,
    direction: Direction,
    depth: u64,
    discovered: DiscoveredTracker,
    relationships: &'a HashSet<Relationship>,
    connection: &'a C,
}

impl<'a, C: ConnectionTrait> Collector<'a, C> {
    /// Create a new collector, with a new visited set.
    fn new(
        graphs: &'a [(String, Arc<PackageGraph>)],
        graph: &'a Graph<graph::Node, Relationship, petgraph::Directed>,
        node: NodeIndex,
        direction: Direction,
        depth: u64,
        relationships: &'a HashSet<Relationship>,
        connection: &'a C,
    ) -> Self {
        Self {
            graphs,
            graph,
            node,
            direction,
            depth,
            discovered: Default::default(),
            relationships,
            connection,
        }
    }

    /// Continue with another graph and node as an entry point.
    ///
    /// Shares the visited set.
    fn with(
        self,
        graph: &'a Graph<graph::Node, Relationship, petgraph::Directed>,
        node: NodeIndex,
    ) -> Self {
        Self {
            graph,
            node,
            ..self
        }
    }

    /// Continue with a new node, but the same graph.
    ///
    /// Decreases depth by one and keeps the visited set.
    fn continue_node(&self, node: NodeIndex) -> Self {
        Self {
            graphs: self.graphs,
            graph: self.graph,
            node,
            direction: self.direction,
            depth: self.depth - 1,
            discovered: self.discovered.clone(),
            relationships: self.relationships,
            connection: self.connection,
        }
    }

    /// Collect related nodes in the provided direction.
    ///
    /// If the depth is zero, or the node was already processed, it will return [`None`], indicating
    /// that the request was not processed.
    async fn collect(self) -> Option<Vec<Node>> {
        tracing::debug!(direction = ?self.direction, "collecting for {:?}", self.node);

        if self.depth == 0 {
            log::debug!("depth is zero");
            // we ran out of depth
            return None;
        }

        if !self.discovered.visit(self.graph, self.node) {
            log::debug!("node got visited already");
            // we've already seen this
            return None;
        }

        match self.graph.node_weight(self.node) {
            // collect external sbom ref
            Some(External(external_node)) => {
                let (external_sbom_id, external_node_id) =
                    resolve_external_sbom(external_node.node_id.clone(), self.connection).await?;

                log::warn!("external sbom id: {:?}", external_sbom_id);
                log::warn!("external node id: {:?}", external_node_id);

                // get external sbom graph
                let Some((_, external_graph)) = self
                    .graphs
                    .iter()
                    .find(|(sbom_id, _)| sbom_id == &external_sbom_id.to_string())
                else {
                    log::warn!("Graph not found.");
                    return None;
                };

                // now that we have the graph, find the external node reference in that graph
                // so we have a starting point.
                let Some(external_node_index) = external_graph
                    .node_indices()
                    .find(|&node| external_graph[node].node_id.eq(&external_node_id))
                else {
                    log::warn!("Node with ID {} not found", external_node_id);
                    // You can return early, log an error, or take other actions as needed
                    return None;
                };

                // process as normal, which is just non-DRY of following code block which we
                // can optimise away.
                log::warn!("external node index: {:?}", external_node_index);

                Some(
                    self.with(external_graph, external_node_index)
                        .collect_graph()
                        .await,
                )
            }
            // collect
            _ => Some(self.collect_graph().await),
        }
    }

    async fn collect_graph(&self) -> Vec<Node> {
        let mut result = vec![];

        for edge in self.graph.edges_directed(self.node, self.direction) {
            log::debug!("edge {edge:?}");

            // we only recurse in one direction
            let (ancestor, descendent, package_node) = match self.direction {
                Direction::Incoming => (
                    Box::pin(self.continue_node(edge.source()).collect()).await,
                    None,
                    self.graph.node_weight(edge.source()),
                ),
                Direction::Outgoing => (
                    None,
                    Box::pin(self.continue_node(edge.target()).collect()).await,
                    self.graph.node_weight(edge.target()),
                ),
            };

            let relationship = edge.weight();

            if !self.relationships.is_empty() && !self.relationships.contains(relationship) {
                // if we have entries, and no match, continue with the next
                continue;
            }

            let Some(package_node) = package_node else {
                continue;
            };

            result.push(Node {
                base: BaseSummary::from(package_node),
                relationship: Some(*relationship),
                ancestors: ancestor,
                descendants: descendent,
            });
        }

        result
    }
}

impl AnalysisService {
    /// Create a new analysis service instance with the configured cache size.
    ///
    /// ## Caching
    ///
    /// A new instance will have a new cache. Instanced cloned from it, will share that cache.
    ///
    /// Therefore, it is ok to create a new instance. However, if you want to make use of the
    /// caching, it is necessary to re-use that instance.
    ///
    /// Also, we do not implement default because of this. As a new instance has the implication
    /// of having its own cache. So creating a new instance should be a deliberate choice.
    pub fn new(config: AnalysisConfig) -> Self {
        let graph_cache = Arc::new(GraphMap::new(config.max_cache_size.as_u64()));

        let meter = global::meter("AnalysisService");
        {
            let graph_cache = graph_cache.clone();
            meter
                .u64_observable_gauge("cache_size")
                .with_callback(move |inst| inst.observe(graph_cache.size_used(), &[]))
                .build();
        };
        {
            let graph_cache = graph_cache.clone();
            meter
                .u64_observable_gauge("cache_items")
                .with_callback(move |inst| inst.observe(graph_cache.len(), &[]))
                .build();
        };

        Self { graph_cache }
    }

    pub fn cache_size_used(&self) -> u64 {
        self.graph_cache.size_used()
    }

    pub fn cache_len(&self) -> u64 {
        self.graph_cache.len()
    }

    #[instrument(skip_all, err)]
    pub async fn load_all_graphs<C: ConnectionTrait>(
        &self,
        connection: &C,
    ) -> Result<Vec<(String, Arc<PackageGraph>)>, Error> {
        // retrieve all sboms in trustify

        let distinct_sbom_ids = sbom::Entity::find()
            .select()
            .order_by(sbom::Column::DocumentId, Order::Asc)
            .order_by(sbom::Column::Published, Order::Desc)
            .all(connection)
            .await?
            .into_iter()
            .map(|record| record.sbom_id.to_string()) // Assuming sbom_id is of type String
            .collect();

        self.load_graphs(connection, &distinct_sbom_ids).await
    }

    pub fn clear_all_graphs(&self) -> Result<(), Error> {
        self.graph_cache.clear();
        Ok(())
    }

    pub async fn status<C: ConnectionTrait>(
        &self,
        connection: &C,
    ) -> Result<AnalysisStatus, Error> {
        let distinct_sbom_ids = sbom::Entity::find()
            .select()
            .order_by(sbom::Column::DocumentId, Order::Asc)
            .order_by(sbom::Column::Published, Order::Desc)
            .all(connection)
            .await?;

        Ok(AnalysisStatus {
            sbom_count: distinct_sbom_ids.len() as u32,
            graph_count: self.graph_cache.len() as u32,
        })
    }

    /// Collect nodes from the graph
    #[instrument(skip(self, create))]
    async fn collect_graph<'a, C>(
        &self,
        query: impl Into<GraphQuery<'a>> + Debug,
        graphs: &[(String, Arc<PackageGraph>)],
        create: C,
    ) -> Vec<Node>
    where
        C: AsyncFn(&Graph<graph::Node, Relationship>, NodeIndex, &graph::Node) -> Node,
    {
        let query = query.into();

        stream::iter(
            graphs
                .iter()
                .filter(|(sbom_id, graph)| acyclic(sbom_id, graph)),
        )
        .flat_map(|(_, graph)| {
            stream::iter(
                graph
                    .node_indices()
                    .filter(|&i| Self::filter(graph, &query, i))
                    .filter_map(|i| graph.node_weight(i).map(|w| (i, w))),
            )
            .then(|(node_index, package_node)| create(graph, node_index, package_node))
        })
        .collect::<Vec<_>>()
        .await
    }

    #[instrument(skip(self, connection))]
    pub async fn run_graph_query<'a, C: ConnectionTrait>(
        &self,
        query: impl Into<GraphQuery<'a>> + Debug,
        options: QueryOptions,
        graphs: &[(String, Arc<PackageGraph>)],
        connection: &C,
    ) -> Vec<Node> {
        let relationships = options.relationships;

        self.collect_graph(query, graphs, async |graph, node_index, node| {
            log::debug!(
                "Discovered node - sbom: {}, node: {}",
                node.sbom_id,
                node.node_id
            );
            Node {
                base: node.into(),
                relationship: None,
                ancestors: Box::pin(
                    Collector::new(
                        graphs,
                        graph,
                        node_index,
                        Direction::Incoming,
                        options.ancestors,
                        &relationships,
                        connection,
                    )
                    .collect(),
                )
                .await,
                descendants: Box::pin(
                    Collector::new(
                        graphs,
                        graph,
                        node_index,
                        Direction::Outgoing,
                        options.descendants,
                        &relationships,
                        connection,
                    )
                    .collect(),
                )
                .await,
            }
        })
        .await
    }

    /// locate components, retrieve dependency information, from a single SBOM
    #[instrument(skip(self, connection), err)]
    pub async fn retrieve_single<C: ConnectionTrait>(
        &self,
        sbom_id: Uuid,
        query: impl Into<GraphQuery<'_>> + Debug,
        options: impl Into<QueryOptions> + Debug,
        paginated: Paginated,
        connection: &C,
    ) -> Result<PaginatedResults<Node>, Error> {
        let distinct_sbom_ids = vec![sbom_id.to_string()];

        let query = query.into();
        let options = options.into();

        let graphs = self.load_graphs(connection, &distinct_sbom_ids).await?;
        let components = self
            .run_graph_query(query, options, &graphs, connection)
            .await;

        Ok(paginated.paginate_array(&components))
    }

    /// locate components, retrieve dependency information
    #[instrument(skip(self, connection), err)]
    pub async fn retrieve<C: ConnectionTrait>(
        &self,
        query: impl Into<GraphQuery<'_>> + Debug,
        options: impl Into<QueryOptions> + Debug,
        paginated: Paginated,
        connection: &C,
    ) -> Result<PaginatedResults<Node>, Error> {
        let query = query.into();
        let options = options.into();

        let graphs = self.load_graphs_query(connection, query).await?;
        let components = self
            .run_graph_query(query, options, &graphs, connection)
            .await;

        Ok(paginated.paginate_array(&components))
    }

    /// check if a node in the graph matches the provided query
    fn filter(graph: &Graph<graph::Node, Relationship>, query: &GraphQuery, i: NodeIndex) -> bool {
        match query {
            GraphQuery::Component(ComponentReference::Id(component_id)) => graph
                .node_weight(i)
                .is_some_and(|node| node.node_id.eq(component_id)),
            GraphQuery::Component(ComponentReference::Name(component_name)) => graph
                .node_weight(i)
                .is_some_and(|node| node.name.eq(component_name)),
            GraphQuery::Component(ComponentReference::Purl(purl)) => {
                graph.node_weight(i).is_some_and(|node| match node {
                    graph::Node::Package(package) => package.purl.contains(purl),
                    _ => false,
                })
            }
            GraphQuery::Component(ComponentReference::Cpe(cpe)) => {
                graph.node_weight(i).is_some_and(|node| match node {
                    graph::Node::Package(package) => package.cpe.contains(cpe),
                    _ => false,
                })
            }
            GraphQuery::Query(query) => graph.node_weight(i).is_some_and(|node| {
                let mut context = HashMap::from([
                    ("sbom_id", Value::String(&node.sbom_id)),
                    ("node_id", Value::String(&node.node_id)),
                    ("name", Value::String(&node.name)),
                ]);
                match node {
                    graph::Node::Package(package) => {
                        context.extend([("version", Value::String(&package.version))]);
                    }
                    graph::Node::External(external) => {
                        context.extend([
                            (
                                "external_document_reference",
                                Value::String(&external.external_document_reference),
                            ),
                            (
                                "external_node_id",
                                Value::String(&external.external_node_id),
                            ),
                        ]);
                    }
                    _ => {}
                }
                query.apply(&context)
            }),
        }
    }
}

fn acyclic(id: &str, graph: &Arc<PackageGraph>) -> bool {
    use petgraph::visit::{DfsEvent, depth_first_search};
    let g = graph.as_ref();
    let result = depth_first_search(g, g.node_identifiers(), |event| match event {
        DfsEvent::BackEdge(source, target) => Err((source, target)),
        _ => Ok(()),
    })
    .err();
    if let Some((start, end)) = result {
        // FIXME: we need a better strategy handling such errors
        let start = graph.node_weight(start);
        let end = graph.node_weight(end);
        log::warn!(
            "analysis graph of sbom {id} has circular references (detected: {start:?} -> {end:?})!",
        );
    }
    result.is_none()
}
