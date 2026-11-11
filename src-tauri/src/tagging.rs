//! Automatic tags: keyword rules matched against file and folder names, plus a few tags that
//! follow from the analysis (loops, short hits, long beds).

use std::collections::BTreeSet;

use crate::models::TagRule;

/// Words in file names that mark a loop.
const LOOP_WORDS: &[&str] = &["loop", "loops", "looped", "loopable", "lp", "seamless"];

pub fn default_rules() -> Vec<TagRule> {
    let rules: &[(&str, &str)] = &[
        ("explosion", "explosion, explode, boom, blast, detonation, bomb, grenade, kaboom, nuke"),
        ("impact", "impact, hit, punch, smack, thud, slam, bump, crash, knock, kick, slap"),
        ("footstep", "footstep, footsteps, step, steps, walk, walking, running, jog, foot"),
        (
            "weapon",
            "gun, gunshot, rifle, pistol, shot, shotgun, reload, sword, blade, weapon, bow, arrow, cannon",
        ),
        ("laser", "laser, blaster, phaser, raygun, zap"),
        (
            "ui",
            "ui, click, button, menu, hover, select, confirm, cancel, notification, interface, tick, toggle",
        ),
        ("beep", "beep, blip, bleep, chirp"),
        ("magic", "magic, spell, cast, enchant, heal, mana, fairy, sparkle, arcane, shimmer, chime"),
        ("ambience", "ambience, ambient, atmosphere, atmos, amb, roomtone, room tone, background, drone"),
        ("nature", "nature, wind, forest, bird, birds, leaves, insects, crickets, jungle"),
        ("weather", "rain, thunder, storm, wind, hail"),
        (
            "water",
            "water, splash, drip, bubble, bubbles, underwater, liquid, pour, river, ocean, wave, waves, stream",
        ),
        ("fire", "fire, flame, flames, burn, burning, torch, campfire, fireball"),
        ("creature", "monster, creature, growl, roar, zombie, beast, dragon, alien, snarl, hiss"),
        ("animal", "animal, dog, cat, horse, cow, bird, wolf, pig, sheep, chicken, bark, meow"),
        (
            "voice",
            "voice, vocal, scream, yell, shout, laugh, grunt, breath, speech, talk, whisper, cough, pain, death",
        ),
        (
            "vehicle",
            "car, engine, vehicle, truck, motorbike, bike, plane, helicopter, train, horn, tires, spaceship",
        ),
        (
            "mechanical",
            "machine, mechanical, mechanism, gear, gears, servo, robot, hydraulic, pneumatic, motor, lever",
        ),
        ("door", "door, doors, gate, unlock, latch, hinge, hatch"),
        ("whoosh", "whoosh, swoosh, swish, swoop, woosh, swing, flyby, fly by, pass by, passby"),
        ("pickup", "pickup, coin, coins, collect, item, powerup, power up, bonus, reward, gem, loot"),
        ("glass", "glass, shatter, window, bottle"),
        ("metal", "metal, metallic, clang, clank, anvil, chain, chains"),
        ("wood", "wood, wooden, timber, stick, plank, crate"),
        ("stone", "stone, rock, rocks, rubble, gravel, debris, brick"),
        ("electric", "electric, electricity, spark, sparks, buzz, static, shock, tesla, electrical"),
        ("music", "music, jingle, stinger, melody, chord, riff, theme, sting, fanfare"),
        ("scifi", "scifi, sci fi, futuristic, teleport, beam, warp, hologram, forcefield, tech"),
        ("retro", "8bit, 8 bit, chiptune, retro, arcade, chip, 16bit, 16 bit, nes, gameboy"),
        ("cartoon", "cartoon, toon, comic, boing, funny, silly, squeak, pop, slide whistle"),
        ("horror", "horror, scary, creepy, spooky, eerie, ghost, haunted, tension"),
        ("foley", "foley, cloth, fabric, rustle, clothes, paper, handling, grab, drop"),
        ("crowd", "crowd, cheer, applause, audience, people, chatter, walla"),
        ("alarm", "alarm, siren, warning, alert, buzzer"),
        ("transition", "transition, riser, rise, downlifter, uplifter, sweep, reverse, braam"),
        ("success", "success, win, victory, levelup, level up, complete, achievement, correct"),
        ("failure", "fail, failure, lose, error, wrong, game over, gameover, denied"),
    ];
    rules
        .iter()
        .map(|(tag, keywords)| TagRule {
            tag: tag.to_string(),
            keywords: keywords.split(',').map(|k| k.trim().to_string()).collect(),
        })
        .collect()
}

/// Splits a file or folder name into lower-case words: "BigDoor_Slam-02" → big, door, slam.
/// Pure numbers are dropped; "8bit" stays one word.
pub fn words(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut word = String::new();
    let mut prev: Option<char> = None;
    let flush = |word: &mut String, out: &mut Vec<String>| {
        if !word.is_empty() && !word.chars().all(|c| c.is_ascii_digit()) {
            out.push(word.to_lowercase());
        }
        word.clear();
    };
    for c in text.chars() {
        if !c.is_alphanumeric() {
            flush(&mut word, &mut out);
            prev = None;
            continue;
        }
        // camelCase boundary, and letters after numbers ("02Slam"), but not "8bit".
        let boundary = match prev {
            Some(p) => (p.is_lowercase() && c.is_uppercase()) || (p.is_ascii_digit() && c.is_uppercase()),
            None => false,
        };
        if boundary {
            flush(&mut word, &mut out);
        }
        word.push(c);
        prev = Some(c);
    }
    flush(&mut word, &mut out);
    out
}

fn singular(word: &str) -> &str {
    if word.len() > 4 && word.ends_with("es") && !word.ends_with("ses") {
        &word[..word.len() - 2]
    } else if word.len() > 3 && word.ends_with('s') && !word.ends_with("ss") {
        &word[..word.len() - 1]
    } else {
        word
    }
}

/// Whether one of the words matches the keyword. Keywords of several words must appear in
/// order; long keywords also match longer words ("explosion" in "explosions_big").
fn matches(words: &[String], joined: &str, keyword: &str) -> bool {
    let keyword = keyword.trim().to_lowercase();
    if keyword.is_empty() {
        return false;
    }
    if keyword.contains(' ') {
        return joined.contains(&format!(" {keyword} "));
    }
    words
        .iter()
        .any(|w| w == &keyword || singular(w) == keyword || (keyword.len() >= 5 && w.starts_with(&keyword)))
}

/// What the tagger knows about a file.
pub struct TagInput<'a> {
    pub name: &'a str,
    /// Folder relative to the library folder.
    pub dir: &'a str,
    pub duration: f64,
    pub attack: f64,
    pub loop_kind: Option<&'a str>,
}

/// Automatic tags and whether the name marks a loop.
pub fn auto_tags(input: &TagInput, rules: &[TagRule], folder_tags: bool) -> (Vec<String>, bool) {
    let mut words = words(input.name);
    let dir_words = words_in_dir(input.dir);
    words.extend(dir_words.iter().cloned());
    let joined = format!(" {} ", words.join(" "));
    let mut tags = BTreeSet::new();
    for rule in rules {
        let tag = normalize_tag(&rule.tag);
        if !tag.is_empty() && rule.keywords.iter().any(|k| matches(&words, &joined, k)) {
            tags.insert(tag);
        }
    }
    if folder_tags {
        // The folder the file is in, as one tag ("Door Slams" → "door slams").
        if let Some(last) = input.dir.rsplit('/').find(|s| !s.is_empty()) {
            let tag = normalize_tag(&self::words(last).join(" "));
            if !tag.is_empty() {
                tags.insert(tag);
            }
        }
    }
    let name_words = self::words(input.name);
    let looped_by_name = name_words.iter().any(|w| LOOP_WORDS.contains(&w.as_str()));
    if looped_by_name || input.loop_kind.is_some() {
        tags.insert("loop".into());
    } else if input.duration > 0.0 && input.duration < 1.5 && input.attack < 0.03 {
        tags.insert("one-shot".into());
    }
    if input.duration >= 30.0 {
        tags.insert("long".into());
    }
    (tags.into_iter().collect(), looped_by_name)
}

fn words_in_dir(dir: &str) -> Vec<String> {
    dir.split('/').flat_map(words).collect()
}

/// Tags are lower case with single spaces.
pub fn normalize_tag(tag: &str) -> String {
    tag.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tags_of(name: &str, dir: &str) -> Vec<String> {
        let input = TagInput { name, dir, duration: 2.0, attack: 0.2, loop_kind: None };
        auto_tags(&input, &default_rules(), false).0
    }

    #[test]
    fn splits_names_into_words() {
        assert_eq!(words("BigDoor_Slam-02"), ["big", "door", "slam"]);
        assert_eq!(words("8bit coin PICKUP 3"), ["8bit", "coin", "pickup"]);
        assert_eq!(words("explosion02Large"), ["explosion02", "large"]);
        assert_eq!(words("Füße laufen"), ["füße", "laufen"]);
    }

    #[test]
    fn tags_by_keywords_in_name_and_folder() {
        assert_eq!(tags_of("Explosions_Big_01", ""), ["explosion"]);
        assert_eq!(tags_of("whoosh 3", "Weapons/Swords"), ["weapon", "whoosh"]);
        assert_eq!(tags_of("stone_footsteps", ""), ["footstep", "stone"]);
        assert_eq!(tags_of("room tone office", ""), ["ambience"]);
        // "hit" must not match "white"; short keywords need whole words.
        assert!(tags_of("white noise", "").is_empty());
    }

    #[test]
    fn analysis_tags() {
        let rules = default_rules();
        let hit = TagInput { name: "x", dir: "", duration: 0.4, attack: 0.001, loop_kind: None };
        assert_eq!(auto_tags(&hit, &rules, false).0, ["one-shot"]);
        let bed = TagInput { name: "Forest_Loop", dir: "", duration: 60.0, attack: 1.0, loop_kind: None };
        let (tags, by_name) = auto_tags(&bed, &rules, false);
        assert_eq!(tags, ["long", "loop", "nature"]);
        assert!(by_name);
        let seamless =
            TagInput { name: "hum", dir: "", duration: 3.0, attack: 0.0, loop_kind: Some("seamless") };
        assert_eq!(auto_tags(&seamless, &rules, false).0, ["loop"]);
    }

    #[test]
    fn folder_tags_are_optional() {
        let input =
            TagInput { name: "a", dir: "Pack/Door Slams", duration: 2.0, attack: 0.5, loop_kind: None };
        assert_eq!(auto_tags(&input, &default_rules(), true).0, ["door", "door slams", "impact"]);
    }
}
