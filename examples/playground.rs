use regex::Regex;

fn main() {
    let content = "<think>\n\n\nIt's part of the logic to handle frame seeking or playback when the exact \
                   frame isn't available, possibly due to decoding not being caught up or seeking in \
                   progress.\n</think>or seeking in progress.";

    let re = Regex::new(r"(?s)<think>(.*?)</think>")
        .ok()
        .unwrap();
    re.captures(&content)
        .and_then(|caps| {
            let thinking_part = caps
                .get(1)
                .map(|m| m.as_str());
            // if let Some(thinking) = thinking_part {
            //     content = content
            //         .replace(&format!("{}", thinking), "")
            //         .as_str(); // keep tags in place
            //                    // .trim()
            // }
            dbg!(thinking_part);
            thinking_part.map(|s| {
                s /*.trim()*/
                    .to_string()
            })
        })
        .unwrap();

    // Ok(())
}
