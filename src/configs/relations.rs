use std::{cmp::Ordering, collections::HashSet, iter::Filter};

use strum_macros::Display;

pub trait Node {
    fn tags(&self) -> &Vec<String>;
    fn id(&self) -> &String;
}

type RelationID = String;

type Reference = (Query, Query);

struct System<NodeType>
where
    for<'a> &'a NodeType: Node,
{
    nodes: Vec<NodeType>,
    relations: Vec<(Reference, RelationID)>,
}

impl<NodeType> System<NodeType>
where
    for<'a> &'a NodeType: Node,
{
    fn query<Resolver, OutputType>(&self, q: Query) -> Vec<OutputType>
    where
        Resolver: RelationResolver<NodeType, OutputType>,
        OutputType: Default,
        // wow first time actually using hrtb in production
        for<'a> &'a NodeType: Node,
    {
        self.nodes
            .iter()
            .filter_query(&q)
            .map(|node_a| {
                let initial = OutputType::default();
                self.relations
                    .iter()
                    .filter(|((qa, _qb), _rel)| qa.matches(&node_a))
                    .flat_map(|((_qa, qb), rel)| {
                        self.nodes.iter().filter_query(qb).map(move |x| (x, rel))
                    })
                    .fold(initial, |cum, (node_b, rel)| {
                        Resolver::resolve(&rel)(node_a, node_b, cum)
                    })
            })
            .collect::<Vec<_>>()
    }
}

trait RelationResolver<NodeType, OutputType> {
    fn resolve<'a>(
        name: &RelationID,
    ) -> &'a dyn Fn(&'a NodeType, &'a NodeType, OutputType) -> OutputType;
}

// Something which gets applied on a node pair if relation exists
type RelationProperty<'a, F, T> = &'static dyn (Fn(&'a F, &'a T) -> &'a F);

#[derive(Clone, Debug, Display)]
enum Query {
    And(Box<Query>, Box<Query>),
    Or(Box<Query>, Box<Query>),
    Not(Box<Query>),
    HasID(String),
    HasTag(String),
    All(),
}
impl Query {
    pub fn matches(&self, item: &dyn Node) -> bool {
        match self {
            Query::And(a, b) => a.matches(item) && b.matches(item),
            Query::Or(a, b) => a.matches(item) || b.matches(item),
            Query::Not(q) => !q.matches(item),
            Query::HasID(id) => item.id().eq(id),
            Query::HasTag(tag) => item.tags().contains(&tag),
            Query::All() => true,
        }
    }
}

struct QueryIter<Iter> {
    q: Query,
    i: Iter,
}

impl<Iter> Iterator for QueryIter<Iter>
where
    Iter: Iterator,
    <Iter as Iterator>::Item: Node,
{
    type Item = <Iter as Iterator>::Item;

    fn next(&mut self) -> Option<Self::Item> {
        while let Some(f) = self.i.next() {
            if self.q.matches(&f) {
                return Some(f);
            }
        }
        None
    }
}

trait FilterNodes<T>
where
    Self: Sized,
    Self: Iterator<Item = T>,
    T: Node,
{
    fn filter_query(self, q: &Query) -> QueryIter<Self> {
        QueryIter {
            i: self,
            q: q.clone(),
        }
    }
}
impl<Iter> FilterNodes<<Iter as Iterator>::Item> for Iter
where
    Iter: Iterator,
    <Iter as Iterator>::Item: Node,
{
}

struct TNode {
    data: u64,
    tags: Vec<String>,
    id: String,
}

impl Node for &TNode {
    fn tags(&self) -> &Vec<String> {
        &self.tags
    }

    fn id(&self) -> &String {
        &self.id
    }
}

struct TestResolver;

impl RelationResolver<TNode, Vec<(String, String)>> for TestResolver {
    fn resolve<'a>(
        _name: &RelationID,
    ) -> &'a dyn Fn(&'a TNode, &'a TNode, Vec<(String, String)>) -> Vec<(String, String)> {
        &crate::configs::relations::f
    }
}

fn f(a: &TNode, b: &TNode, c: Vec<(String, String)>) -> Vec<(String, String)> {
    let mut c = c.clone();
    c.push((a.id.clone(), b.id.clone()));
    c
}

#[test]
pub fn test_queries() {
    let system = System {
        nodes: vec![
            TNode {
                data: 12,
                tags: vec!["desktop".to_string()],
                id: "tiferet".to_string(),
            },
            TNode {
                data: 12,
                tags: vec!["net:tiferet".to_string(), "notebook".to_string()],
                id: "a".to_string(),
            },
            TNode {
                data: 42,
                tags: vec!["net:tiferet".to_string(), "mobile".to_string()],
                id: "b".to_string(),
            },
            TNode {
                data: 34,
                tags: vec!["net:tiferet".to_string(), "mobile".to_string()],
                id: "c".to_string(),
            },
        ],
        relations: vec![
            (
                (
                    Query::HasID("tiferet".to_string()),
                    Query::HasTag("net:tiferet".to_string()),
                ),
                "provide-net".to_string(),
            ),
            (
                (
                    Query::HasTag("net:tiferet".to_string()),
                    Query::HasID("tiferet".to_string()),
                ),
                "host".to_string(),
            ),
        ],
    };

    // let f: std::slice::Iter<TNode> = vec.iter();
    // let r: std::vec::IntoIter<TNode> = vec.into_iter();

    println!("{:?}", system.query::<TestResolver, _>(Query::All()));
}

// // Something getting applied on a node if
// type NodeProperty<F, T> = dyn (Fn(F) -> F) + 'static;
