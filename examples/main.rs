extern crate lintparser;
use lintparser::Check;

fn main() {
    let res = lintparser::cargo_check().expect("Oh noes!");
    match res {
        Check::Perfect => {
            println!("No problems found");
        },
        Check::Warning(ref problems) => {
            println!("Warning:");
            for problem in problems {
                println!("- {}", problem);
            }
        },
        Check::Error(ref problems) => {
            println!("Error:");
            for problem in problems {
                println!("- {}", problem);
            }
        },
    }
}