# Egg Optimizer Design

We need to build something and run some tests that actually push it to the limit.
Let's begin with a ruleset we know and love that has some difficult tests: mathbench.

We'll start with arithmetic simplification, cost is AST size.

We will use `egg` as the memo backend (and a point of comparison) but expose fine-grained control over exploration.
With that control, we will implement a variety of different search strategies over the common backend/term/rule infra.
Given that, we can then develop some challenging test cases and benchmarks and try the approaches against one another.

## Overview

### System Components

1. Modify egg to expose fine-grained control over rewriting (apply this rule to this node, and build up form there)
2. Design a generic `Explorer` interface with a task queue and task handlers. We can implement any search strategy as a `Explorer` and provide support for common operations and performance measurements.
3. Instrument for performance tracking - we want to compare node counts (over time) and make it easy to add more metrics and things later on.
4. Build a testing frontend for easy test running, formatted data, and graphics.
5. Implement the search strategies.

### Search Approaches to Test

## Implementation

### Metrics

- [ ] Decide on a metrics interface: `metrics` + `rerun`??
- [ ] Instrument `egg` to emit metrics when necessary

### `egg` Changes

Expose fine grained control over exploration.

- [ ] Give nodes fixed `Id`s so we can refer to them
- [ ] Interface to apply a single rewrite rule to a single node
- [ ] (Optional for now) make rebuilding configurable (only rebuild so much)

### `Explorer` Interface

Should be a fairly generic setup where the actual work is required by the trait.
It will provide implementations for common things (like a rebuild task) and handle the usual initialization, metric, setup bullshit.

`SearchStrategy<T>` trait for task type `T`:

- [ ] `process_task(&mut self, task: T)` Task handler for executing search tasks (mut self ref)

`Explorer` will be generic over language, cost, analysis and must implement `Search`

- [ ] `init` create an e-graph, seed an initial term
- [ ] `with_metrics()` init metrics recorder
- [ ] `with_rules()` to add more rules (phasing)
- [ ] `with_task_limit()`
- [ ] `with_time_limit()`
- [ ] `extract()` Extract optimized term
- [ ] `pop_task(&mut self) -> T` Task queue structure (with a default override-able implementation)
- [ ] `push_task(&mut self, task: T)` Push a task onto the queue (in some order)

`Runner` trait is auto derived for an Explorer that implements searcher

- [ ] `run()` Run until task queue or limit is reached
