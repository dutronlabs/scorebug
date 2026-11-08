use scorebug_core::Team;

/// (abbr, name, primary color). Colors are approximate placeholders; real
/// providers supply the authoritative values.
const TEAMS: [(&str, &str, &str); 32] = [
    ("ANA", "Anaheim Ducks", "#F47A38"),
    ("BOS", "Boston Bruins", "#FFB81C"),
    ("BUF", "Buffalo Sabres", "#003087"),
    ("CGY", "Calgary Flames", "#D2001C"),
    ("CAR", "Carolina Hurricanes", "#CC0000"),
    ("CHI", "Chicago Blackhawks", "#CF0A2C"),
    ("COL", "Colorado Avalanche", "#6F263D"),
    ("CBJ", "Columbus Blue Jackets", "#002654"),
    ("DAL", "Dallas Stars", "#006847"),
    ("DET", "Detroit Red Wings", "#CE1126"),
    ("EDM", "Edmonton Oilers", "#FF4C00"),
    ("FLA", "Florida Panthers", "#C8102E"),
    ("LAK", "Los Angeles Kings", "#A2AAAD"),
    ("MIN", "Minnesota Wild", "#154734"),
    ("MTL", "Montreal Canadiens", "#AF1E2D"),
    ("NSH", "Nashville Predators", "#FFB81C"),
    ("NJD", "New Jersey Devils", "#CE1126"),
    ("NYI", "New York Islanders", "#00539B"),
    ("NYR", "New York Rangers", "#0038A8"),
    ("OTT", "Ottawa Senators", "#C52032"),
    ("PHI", "Philadelphia Flyers", "#F74902"),
    ("PIT", "Pittsburgh Penguins", "#FFB81C"),
    ("SJS", "San Jose Sharks", "#006D75"),
    ("SEA", "Seattle Kraken", "#99D9D9"),
    ("STL", "St. Louis Blues", "#002F87"),
    ("TBL", "Tampa Bay Lightning", "#002868"),
    ("TOR", "Toronto Maple Leafs", "#00205B"),
    ("UTA", "Utah Mammoth", "#6CACE4"),
    ("VAN", "Vancouver Canucks", "#00843D"),
    ("VGK", "Vegas Golden Knights", "#B4975A"),
    ("WSH", "Washington Capitals", "#C8102E"),
    ("WPG", "Winnipeg Jets", "#041E42"),
];

pub fn all() -> Vec<Team> {
    TEAMS
        .iter()
        .map(|(abbr, name, color)| Team {
            id: (*abbr).into(),
            abbr: (*abbr).into(),
            name: (*name).into(),
            color: Some((*color).into()),
            logo: None,
        })
        .collect()
}
