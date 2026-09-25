mod analysis;
mod demo;
mod extract;
mod querylang;
mod rules;

use demo::Demo;

fn main() {
    let demos = [
        Demo::new("selection", "(SELECT (SCAN (T- emp)) (C- salary))"),
        Demo::new(
            "join",
            "(JOIN (SCAN (T- emp)) (SCAN (T- dept)) (C- dept_id))",
        ),
        Demo::new(
            "self-join",
            "(JOIN (SCAN (T- emp)) (SCAN (T- emp)) (C- dept_id))",
        ),
        Demo::new(
            "selection over join",
            "(SELECT (JOIN (SCAN (T- emp)) (SCAN (T- dept)) (C- dept_id)) (C- salary))",
        ),
        Demo::new(
            "three-way join",
            "(JOIN (JOIN (SCAN (T- emp)) (SCAN (T- dept)) (C- dept_id)) (SCAN (T- proj)) (C- proj_id))",
        ),
    ];
    for demo in demos {
        println!("{}", demo.run());
    }
}
