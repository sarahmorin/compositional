/// E-graph for the arithmetic ISLE example.
#[allow(dead_code, unused_variables, unused_imports, non_snake_case)]
#[allow(irrefutable_let_patterns, unused_assignments, non_camel_case_types)]
#[allow(unreachable_patterns, unreachable_code)]
#[path = "isle/arith.rs"]
pub(crate) mod arith;

pub type ConstructorVec<T> = Vec<T>;
pub const MAX_ISLE_RETURNS: usize = 1000;

use std::collections::HashMap;

use arith::{Context, *};

use compositional::ast::MinAST;
use compositional::define_language;
use compositional::egraph::core::{ClassId, EGraph, Node, NodeId};
use compositional::egraph::language::{Language, Term};
type Id = ClassId;

use log::debug;

define_language! {
    pub enum Arith {
        // Operators
        "Add" = Add(2),
        "Mul" = Mul(2),

        // Constants
        Num(u32),

        // Variables
        Var(String),
    }
}

pub struct Metrics {
    pub num_nodes: usize,
    pub num_classes: usize,
    pub class_visits: HashMap<ClassId, usize>,
    pub node_visits: HashMap<NodeId, usize>,
}

pub struct IsleRunner {
    pub egraph: EGraph<Arith, MinAST>,
    pub metrics: Metrics,
}

impl IsleRunner {
    pub fn new() -> Self {
        IsleRunner {
            egraph: EGraph::new(MinAST),
            metrics: Metrics {
                num_nodes: 0,
                num_classes: 0,
                class_visits: HashMap::new(),
                node_visits: HashMap::new(),
            },
        }
    }

    pub fn term_from_string(&self, s: &str) -> Term<Arith> {
        let term = s.parse::<Term<Arith>>().unwrap();
        term
    }

    pub fn add_term(&mut self, term: &Term<Arith>) -> Id {
        let (id, _) = self.egraph.add_term(term);
        id.class()
    }

    fn refresh_metrics(&mut self) {
        self.metrics.num_nodes = self.egraph.num_nodes();
        self.metrics.num_classes = self.egraph.num_classes();
    }

    pub fn print_metrics(&mut self) {
        self.refresh_metrics();
        println!("Metrics:");
        println!("  Number of nodes: {}", self.metrics.num_nodes);
        println!("  Number of classes: {}", self.metrics.num_classes);
        println!("  Class visits: {:?}", self.metrics.class_visits);
        println!("  Node visits: {:?}", self.metrics.node_visits);
    }

    pub fn visit_node(&mut self, node_id: NodeId) {
        *self.metrics.node_visits.entry(node_id).or_insert(0) += 1;
    }

    pub fn visit_class(&mut self, class_id: ClassId) {
        *self.metrics.class_visits.entry(class_id).or_insert(0) += 1;
    }
}

impl Context for IsleRunner {
    type extractor_num_returns = ContextIterWrapper<Vec<u32>, Self>;
    type constructor_num_returns = ContextIterWrapper<[Id; 1], Self>;
    type extractor_var_returns = ContextIterWrapper<Vec<String>, Self>;
    type constructor_var_returns = ContextIterWrapper<[Id; 1], Self>;
    type extractor_add_returns = ContextIterWrapper<Vec<(Id, Id)>, Self>;
    type constructor_add_returns = ContextIterWrapper<[Id; 1], Self>;
    type extractor_mul_returns = ContextIterWrapper<Vec<(Id, Id)>, Self>;
    type constructor_mul_returns = ContextIterWrapper<[Id; 1], Self>;

    fn extractor_num(&mut self, arg0: Id, returns: &mut Self::extractor_num_returns) -> () {
        debug!("extractor_num: arg0={:?}", arg0);
        self.visit_class(arg0);
        let class = self.egraph.class(arg0);
        for node_id in &class.nodes {
            let node = self.egraph.get_node(*node_id);
            match &node.root {
                Arith::Num(value) => {
                    debug!("extractor_num: found Num({})", value);
                    returns.push(*value);
                }
                _ => {}
            }
        }
    }

    fn constructor_num(&mut self, arg0: u32, returns: &mut Self::constructor_num_returns) -> () {
        let (id, added) = self.egraph.add_node(Node::new(Arith::Num(arg0), vec![]));
        if added {
            debug!("constructor_num: added new node");
            constructor_new_child(self, id.class(), &mut Vec::new());
        }
        debug!("constructor_num: arg0={}", arg0);
        returns[0] = id.class();
    }

    fn extractor_var(&mut self, arg0: Id, returns: &mut Self::extractor_var_returns) -> () {
        debug!("extractor_var: arg0={:?}", arg0);
        self.visit_class(arg0);
        let class = self.egraph.class(arg0);
        for node_id in &class.nodes {
            let node = self.egraph.get_node(*node_id);
            match &node.root {
                Arith::Var(value) => {
                    debug!("extractor_var: found Var({})", value);
                    returns.push(value.clone());
                }
                _ => {}
            }
        }
    }

    fn constructor_var(&mut self, arg0: String, returns: &mut Self::constructor_var_returns) -> () {
        let (id, added) = self.egraph.add_node(Node::new(Arith::Var(arg0), vec![]));
        if added {
            debug!("constructor_var: added new node");
            constructor_new_child(self, id.class(), &mut Vec::new());
        }
        returns[0] = id.class();
    }

    fn extractor_add(&mut self, arg0: Id, returns: &mut Self::extractor_add_returns) -> () {
        debug!("extractor_add: arg0={:?}", arg0);
        self.visit_class(arg0);
        let nodes = self.egraph.class(arg0).nodes.clone();
        for node_id in nodes {
            self.visit_node(node_id);
            let node = self.egraph.get_node(node_id);
            match &node.root {
                Arith::Add => {
                    debug!(
                        "extractor_add: found Add({:?}, {:?})",
                        node.children[0], node.children[1]
                    );
                    returns.push((node.children[0], node.children[1]));
                }
                _ => {}
            }
        }
    }

    fn constructor_add(
        &mut self,
        arg0: Id,
        arg1: Id,
        returns: &mut Self::constructor_add_returns,
    ) -> () {
        debug!("constructor_add: arg0={:?}, arg1={:?}", arg0, arg1);
        let (id, added) = self
            .egraph
            .add_node(Node::new(Arith::Add, vec![arg0, arg1]));
        if added {
            debug!("constructor_add: added new node");
            constructor_new_child(self, id.class(), &mut Vec::new());
        }
        returns[0] = id.class();
    }

    fn extractor_mul(&mut self, arg0: Id, returns: &mut Self::extractor_mul_returns) -> () {
        debug!("extractor_mul: arg0={:?}", arg0);
        self.visit_class(arg0);
        let class = self.egraph.class(arg0);
        for node_id in &class.nodes {
            let node = self.egraph.get_node(*node_id);
            match &node.root {
                Arith::Mul => {
                    debug!(
                        "extractor_mul: found Mul({:?}, {:?})",
                        node.children[0], node.children[1]
                    );
                    returns.push((node.children[0], node.children[1]));
                }
                _ => {}
            }
        }
    }

    fn constructor_mul(
        &mut self,
        arg0: Id,
        arg1: Id,
        returns: &mut Self::constructor_mul_returns,
    ) -> () {
        debug!("constructor_mul: arg0={:?}, arg1={:?}", arg0, arg1);
        let (id, added) = self
            .egraph
            .add_node(Node::new(Arith::Mul, vec![arg0, arg1]));
        if added {
            debug!("constructor_mul: added new node");
            constructor_new_child(self, id.class(), &mut Vec::new());
        }
        returns[0] = id.class();
    }

    fn constructor_union(&mut self, arg0: Id, arg1: Id) -> Id {
        debug!("constructor_union: arg0={:?}, arg1={:?}", arg0, arg1);
        let _merged = self.egraph.union(arg0, arg1);
        self.egraph.find(arg0)
    }
}

impl IsleRunner
where
    Self: Context,
{
    pub fn run(&mut self, root: Id) {
        constructor_explore(self, root, &mut Vec::new());
    }
}
