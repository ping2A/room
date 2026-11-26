//! Find 0-tic state cycles (next-chain only): each one trips the
//! the endless-loop guard whenever an actor enters it without a jump out.

// Dev tool: failing loudly on bad input is the right behaviour.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use room_vfs::VfsBuilder;
use std::collections::BTreeSet;
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let vfs = room_game::mount_embedded_engine(VfsBuilder::new())
        .unwrap()
        .mount_path(&a[1])
        .unwrap()
        .mount_path(&a[2])
        .unwrap()
        .build()
        .unwrap();
    let data = room_game::load_embedded_engine_data_with(&vfs).unwrap();
    let st = &data.states;
    let mut reported = BTreeSet::new();
    let mut n = 0;
    for i in 0..st.len() {
        let start = room_core::StateId::from_raw(i as u32);
        let mut cur = Some(start);
        let mut seen = Vec::new();
        while let Some(s) = cur {
            let Some(x) = st.get(s) else { break };
            if x.tics != 0 {
                break;
            }
            if let Some(pos) = seen.iter().position(|&q| q == s) {
                let cyc: Vec<_> = seen[pos..].to_vec();
                let key = *cyc
                    .iter()
                    .min_by_key(|q: &&room_core::StateId| q.raw())
                    .unwrap();
                if reported.insert(key) {
                    n += 1;
                    let cls = data
                        .classes
                        .get(x.owner)
                        .map(|c| c.name.clone())
                        .unwrap_or_default();
                    let desc: Vec<String> = cyc
                        .iter()
                        .map(|q| {
                            let y = st.get(*q).unwrap();
                            format!(
                                "{}{}:{}({})",
                                y.sprite,
                                char::from(b'A' + y.frame.min(25)),
                                y.action.as_deref().unwrap_or("-"),
                                y.action_args.join(",")
                            )
                        })
                        .collect();
                    if n <= 60 {
                        println!("{cls}: {}", desc.join(" > "));
                    }
                }
                break;
            }
            seen.push(s);
            cur = x.next;
        }
    }
    println!("0-tic cycles: {n}");
}
