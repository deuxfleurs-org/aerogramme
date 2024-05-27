/// The iCalendar module is not yet properly rewritten
/// Instead we heavily rely on the icalendar library
/// However, for many reason, it's not satisfying:
/// the goal will be to rewrite it in the end so it better
/// integrates into Aerogramme
pub mod parser;
pub mod prune;
pub mod query;
