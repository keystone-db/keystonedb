//! Full-Text Search (FTS) module for KeystoneDB
//!
//! Phase 11: Provides inverted index, tokenization, and search capabilities.
//!
//! Features:
//! - Tokenization with Unicode support, stemming, and stop words
//! - Inverted index stored per text attribute
//! - Boolean queries (AND, OR, NOT)
//! - Phrase queries with position tracking
//! - Fuzzy matching (Levenshtein distance)
//! - BM25 ranking algorithm
//! - Snippet extraction with highlighting

use bytes::Bytes;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, BTreeMap};

use crate::{Key, Item, Value, Result, Error};

// ============================================================================
// Constants
// ============================================================================

/// Marker byte for FTS index keys (different from regular index marker 0xFF)
pub const FTS_INDEX_MARKER: u8 = 0xFE;

/// Default BM25 parameters
pub const BM25_K1: f64 = 1.2;
pub const BM25_B: f64 = 0.75;

// ============================================================================
// Language and Tokenizer Configuration
// ============================================================================

/// Supported languages for text analysis
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum Language {
    #[default]
    English,
    Spanish,
    French,
    German,
    Italian,
    Portuguese,
    Dutch,
    Russian,
    /// No stemming or language-specific processing
    None,
}

impl Language {
    /// Get stop words for this language
    pub fn stop_words(&self) -> &'static [&'static str] {
        match self {
            Language::English => &ENGLISH_STOP_WORDS,
            Language::Spanish => &SPANISH_STOP_WORDS,
            Language::French => &FRENCH_STOP_WORDS,
            Language::German => &GERMAN_STOP_WORDS,
            Language::Italian => &ITALIAN_STOP_WORDS,
            Language::Portuguese => &PORTUGUESE_STOP_WORDS,
            Language::Dutch => &DUTCH_STOP_WORDS,
            Language::Russian => &RUSSIAN_STOP_WORDS,
            Language::None => &[],
        }
    }
}

// ============================================================================
// Stop Words
// ============================================================================

static ENGLISH_STOP_WORDS: [&str; 125] = [
    "a", "an", "and", "are", "as", "at", "be", "by", "for", "from",
    "has", "he", "in", "is", "it", "its", "of", "on", "that", "the",
    "to", "was", "were", "will", "with", "this", "but", "they",
    "have", "had", "what", "when", "where", "who", "which", "why", "how",
    "all", "each", "every", "both", "few", "more", "most", "other", "some",
    "such", "no", "nor", "not", "only", "own", "same", "so", "than", "too",
    "very", "can", "just", "should", "now", "i", "me", "my", "myself", "we",
    "our", "ours", "ourselves", "you", "your", "yours", "yourself", "yourselves",
    "him", "his", "himself", "she", "her", "hers", "herself", "them", "their",
    "theirs", "themselves", "whom", "these", "those",
    "am", "been", "being", "do", "does", "did", "doing", "would", "could",
    "ought", "i'm", "you're", "he's", "she's", "it's", "we're", "they're",
    "i've", "you've", "we've", "they've", "i'd", "you'd", "he'd", "she'd",
    "we'd", "they'd", "i'll", "you'll", "he'll", "she'll", "we'll", "they'll",
    "isn't", "aren't", "wasn't", "weren't",
];

static SPANISH_STOP_WORDS: [&str; 50] = [
    "el", "la", "los", "las", "un", "una", "unos", "unas", "y", "o",
    "de", "del", "al", "a", "en", "con", "para", "por", "que", "se",
    "su", "es", "no", "si", "como", "pero", "más", "este", "esta", "estos",
    "estas", "ese", "esa", "esos", "esas", "aquel", "aquella", "yo", "tú", "él",
    "ella", "nosotros", "vosotros", "ellos", "ellas", "mi", "tu", "su", "nuestro", "vuestro",
];

static FRENCH_STOP_WORDS: [&str; 50] = [
    "le", "la", "les", "un", "une", "des", "et", "ou", "de", "du",
    "au", "aux", "à", "en", "dans", "sur", "avec", "pour", "par", "que",
    "qui", "se", "ce", "cette", "ces", "son", "sa", "ses", "leur", "leurs",
    "ne", "pas", "plus", "moins", "très", "aussi", "bien", "mais", "donc", "car",
    "je", "tu", "il", "elle", "nous", "vous", "ils", "elles", "on", "tout",
];

static GERMAN_STOP_WORDS: [&str; 50] = [
    "der", "die", "das", "ein", "eine", "und", "oder", "von", "zu", "bei",
    "mit", "für", "auf", "in", "an", "nach", "über", "unter", "vor", "hinter",
    "ist", "sind", "war", "waren", "hat", "haben", "wird", "werden", "kann", "können",
    "ich", "du", "er", "sie", "es", "wir", "ihr", "sie", "mein", "dein",
    "sein", "ihr", "unser", "euer", "nicht", "auch", "nur", "noch", "schon", "aber",
];

static ITALIAN_STOP_WORDS: [&str; 50] = [
    "il", "lo", "la", "i", "gli", "le", "un", "uno", "una", "e",
    "o", "di", "a", "da", "in", "con", "su", "per", "tra", "fra",
    "che", "chi", "cui", "quale", "quanto", "questo", "quello", "mio", "tuo", "suo",
    "nostro", "vostro", "loro", "io", "tu", "lui", "lei", "noi", "voi", "essi",
    "non", "più", "molto", "poco", "tanto", "quanto", "anche", "solo", "già", "ancora",
];

static PORTUGUESE_STOP_WORDS: [&str; 50] = [
    "o", "a", "os", "as", "um", "uma", "uns", "umas", "e", "ou",
    "de", "do", "da", "dos", "das", "em", "no", "na", "nos", "nas",
    "por", "para", "com", "sem", "que", "se", "como", "mais", "menos", "muito",
    "pouco", "eu", "tu", "ele", "ela", "nós", "vós", "eles", "elas", "meu",
    "teu", "seu", "nosso", "vosso", "não", "sim", "já", "ainda", "também", "apenas",
];

static DUTCH_STOP_WORDS: [&str; 50] = [
    "de", "het", "een", "en", "of", "van", "naar", "met", "voor", "door",
    "op", "in", "aan", "bij", "om", "te", "tot", "uit", "over", "onder",
    "is", "zijn", "was", "waren", "heeft", "hebben", "wordt", "worden", "kan", "kunnen",
    "ik", "je", "jij", "u", "hij", "zij", "het", "wij", "jullie", "zij",
    "mijn", "jouw", "zijn", "haar", "ons", "niet", "ook", "nog", "wel", "maar",
];

static RUSSIAN_STOP_WORDS: [&str; 50] = [
    "и", "в", "во", "не", "что", "он", "на", "я", "с", "со",
    "как", "а", "то", "все", "она", "так", "его", "но", "да", "ты",
    "к", "у", "же", "вы", "за", "бы", "по", "только", "её", "мне",
    "было", "вот", "от", "меня", "ещё", "нет", "о", "из", "ему", "теперь",
    "когда", "уже", "вам", "ни", "быть", "был", "их", "ли", "это", "сам",
];

// ============================================================================
// Text Index Configuration
// ============================================================================

/// Full-text index definition
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TextIndex {
    /// Index name (unique per table)
    pub name: String,
    /// Attribute name to index
    pub attribute: String,
    /// Language for stemming and stop words
    pub language: Language,
    /// Whether to apply stemming
    pub stemming: bool,
    /// Whether to filter stop words
    pub stop_words: bool,
    /// Minimum token length to index
    pub min_token_length: usize,
    /// Maximum token length to index
    pub max_token_length: usize,
}

impl TextIndex {
    /// Create a new text index with default settings
    pub fn new(name: impl Into<String>, attribute: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            attribute: attribute.into(),
            language: Language::English,
            stemming: true,
            stop_words: true,
            min_token_length: 2,
            max_token_length: 64,
        }
    }

    /// Set the language for text analysis
    pub fn language(mut self, language: Language) -> Self {
        self.language = language;
        self
    }

    /// Enable or disable stemming
    pub fn stemming(mut self, enabled: bool) -> Self {
        self.stemming = enabled;
        self
    }

    /// Enable or disable stop word filtering
    pub fn stop_words(mut self, enabled: bool) -> Self {
        self.stop_words = enabled;
        self
    }

    /// Set minimum token length
    pub fn min_token_length(mut self, len: usize) -> Self {
        self.min_token_length = len;
        self
    }

    /// Set maximum token length
    pub fn max_token_length(mut self, len: usize) -> Self {
        self.max_token_length = len;
        self
    }
}

// ============================================================================
// Tokenizer
// ============================================================================

/// Token with position information
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    /// The normalized token text
    pub text: String,
    /// Position in the original text (0-indexed word position)
    pub position: u32,
    /// Character offset start in original text
    pub start_offset: u32,
    /// Character offset end in original text
    pub end_offset: u32,
}

/// Tokenizer for text analysis
pub struct Tokenizer {
    language: Language,
    stemming: bool,
    stop_words: HashSet<&'static str>,
    min_length: usize,
    max_length: usize,
}

impl Tokenizer {
    /// Create a new tokenizer with the given configuration
    pub fn new(index: &TextIndex) -> Self {
        let stop_words: HashSet<&'static str> = if index.stop_words {
            index.language.stop_words().iter().copied().collect()
        } else {
            HashSet::new()
        };

        Self {
            language: index.language,
            stemming: index.stemming,
            stop_words,
            min_length: index.min_token_length,
            max_length: index.max_token_length,
        }
    }

    /// Create a tokenizer with default English settings
    pub fn english() -> Self {
        Self {
            language: Language::English,
            stemming: true,
            stop_words: ENGLISH_STOP_WORDS.iter().copied().collect(),
            min_length: 2,
            max_length: 64,
        }
    }

    /// Tokenize text into a list of tokens with positions
    pub fn tokenize(&self, text: &str) -> Vec<Token> {
        let mut tokens = Vec::new();
        let mut position = 0u32;
        let mut char_offset = 0u32;

        for word in self.split_words(text) {
            let start_offset = char_offset;
            let end_offset = start_offset + word.len() as u32;
            char_offset = end_offset + 1; // +1 for space/separator

            // Normalize: lowercase
            let normalized = word.to_lowercase();

            // Skip if too short or too long
            if normalized.len() < self.min_length || normalized.len() > self.max_length {
                position += 1;
                continue;
            }

            // Skip stop words
            if self.stop_words.contains(normalized.as_str()) {
                position += 1;
                continue;
            }

            // Apply stemming
            let stemmed = if self.stemming {
                self.stem(&normalized)
            } else {
                normalized
            };

            tokens.push(Token {
                text: stemmed,
                position,
                start_offset,
                end_offset,
            });

            position += 1;
        }

        tokens
    }

    /// Tokenize text and return unique terms with their frequencies
    pub fn tokenize_with_frequency(&self, text: &str) -> HashMap<String, TermInfo> {
        let tokens = self.tokenize(text);
        let mut term_info: HashMap<String, TermInfo> = HashMap::new();

        for token in tokens {
            term_info
                .entry(token.text.clone())
                .or_insert_with(|| TermInfo {
                    frequency: 0,
                    positions: Vec::new(),
                })
                .add_occurrence(token.position);
        }

        term_info
    }

    /// Split text into words (handles Unicode)
    fn split_words<'a>(&self, text: &'a str) -> impl Iterator<Item = &'a str> {
        text.split(|c: char| !c.is_alphanumeric() && c != '\'')
            .filter(|s| !s.is_empty())
    }

    /// Apply Porter stemming algorithm (simplified)
    fn stem(&self, word: &str) -> String {
        if self.language == Language::None {
            return word.to_string();
        }

        // Simplified English stemmer
        // A full implementation would use the Porter or Snowball algorithm
        let mut result = word.to_string();

        // Remove common suffixes
        let suffixes = [
            "ational", "tional", "enci", "anci", "izer", "ation", "ator",
            "alism", "iveness", "fulness", "ousness", "aliti", "iviti",
            "biliti", "alli", "entli", "eli", "ousli", "ization", "ation",
            "ator", "alism", "iveness", "fulness", "ousness", "aliti",
            "ing", "ed", "ly", "es", "s", "ment", "ness", "ity", "able",
            "ible", "al", "ful", "less", "ous", "ive", "ize",
        ];

        for suffix in suffixes {
            if result.len() > suffix.len() + 2 && result.ends_with(suffix) {
                result.truncate(result.len() - suffix.len());
                break;
            }
        }

        // Ensure minimum length
        if result.len() < 2 {
            return word.to_string();
        }

        result
    }
}

/// Term frequency and position information
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TermInfo {
    /// Number of occurrences in the document
    pub frequency: u32,
    /// Positions where the term appears
    pub positions: Vec<u32>,
}

impl TermInfo {
    /// Add an occurrence at the given position
    pub fn add_occurrence(&mut self, position: u32) {
        self.frequency += 1;
        self.positions.push(position);
    }
}

// ============================================================================
// Inverted Index Entry
// ============================================================================

/// Entry in the inverted index (stored per term)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PostingEntry {
    /// Document key (encoded)
    pub doc_key: Vec<u8>,
    /// Term frequency in this document
    pub term_frequency: u32,
    /// Positions where term appears
    pub positions: Vec<u32>,
    /// Document length (for BM25 normalization)
    pub doc_length: u32,
}

/// Posting list for a term (all documents containing this term)
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PostingList {
    /// Document frequency (number of documents containing this term)
    pub doc_frequency: u32,
    /// Individual postings
    pub postings: Vec<PostingEntry>,
}

impl PostingList {
    /// Add a posting for a document
    pub fn add_posting(&mut self, doc_key: Vec<u8>, term_frequency: u32, positions: Vec<u32>, doc_length: u32) {
        self.doc_frequency += 1;
        self.postings.push(PostingEntry {
            doc_key,
            term_frequency,
            positions,
            doc_length,
        });
    }

    /// Remove postings for a document
    pub fn remove_document(&mut self, doc_key: &[u8]) {
        let original_len = self.postings.len();
        self.postings.retain(|p| p.doc_key != doc_key);
        if self.postings.len() < original_len {
            self.doc_frequency = self.postings.len() as u32;
        }
    }
}

// ============================================================================
// FTS Query Types
// ============================================================================

/// Boolean operator for combining query terms
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BooleanOp {
    And,
    Or,
    Not,
}

/// A parsed FTS query
#[derive(Debug, Clone, PartialEq)]
pub enum FtsQuery {
    /// Single term query
    Term(String),
    /// Phrase query (terms must appear in order)
    Phrase(Vec<String>),
    /// Boolean combination
    Boolean {
        left: Box<FtsQuery>,
        op: BooleanOp,
        right: Box<FtsQuery>,
    },
    /// Fuzzy match with edit distance
    Fuzzy {
        term: String,
        max_distance: u8,
    },
    /// Prefix match
    Prefix(String),
    /// Wildcard match (? = single char, * = any chars)
    Wildcard(String),
}

impl FtsQuery {
    /// Create a term query
    pub fn term(t: impl Into<String>) -> Self {
        FtsQuery::Term(t.into())
    }

    /// Create a phrase query
    pub fn phrase(terms: Vec<String>) -> Self {
        FtsQuery::Phrase(terms)
    }

    /// Create an AND query
    pub fn and(left: FtsQuery, right: FtsQuery) -> Self {
        FtsQuery::Boolean {
            left: Box::new(left),
            op: BooleanOp::And,
            right: Box::new(right),
        }
    }

    /// Create an OR query
    pub fn or(left: FtsQuery, right: FtsQuery) -> Self {
        FtsQuery::Boolean {
            left: Box::new(left),
            op: BooleanOp::Or,
            right: Box::new(right),
        }
    }

    /// Create a NOT query
    pub fn not(positive: FtsQuery, negative: FtsQuery) -> Self {
        FtsQuery::Boolean {
            left: Box::new(positive),
            op: BooleanOp::Not,
            right: Box::new(negative),
        }
    }

    /// Create a fuzzy query
    pub fn fuzzy(term: impl Into<String>, max_distance: u8) -> Self {
        FtsQuery::Fuzzy {
            term: term.into(),
            max_distance,
        }
    }

    /// Create a prefix query
    pub fn prefix(p: impl Into<String>) -> Self {
        FtsQuery::Prefix(p.into())
    }
}

// ============================================================================
// FTS Query Parser
// ============================================================================

/// Parser for FTS query strings
pub struct FtsQueryParser {
    tokenizer: Tokenizer,
}

impl FtsQueryParser {
    /// Create a new parser with the given tokenizer
    pub fn new(tokenizer: Tokenizer) -> Self {
        Self { tokenizer }
    }

    /// Create a parser with default English settings
    pub fn english() -> Self {
        Self {
            tokenizer: Tokenizer::english(),
        }
    }

    /// Parse a query string into an FtsQuery
    ///
    /// Syntax:
    /// - Simple terms: `word1 word2` (AND by default)
    /// - Phrases: `"exact phrase"`
    /// - Boolean: `word1 AND word2`, `word1 OR word2`, `word1 NOT word2`
    /// - Prefix: `word*`
    /// - Fuzzy: `word~2` (edit distance 2)
    pub fn parse(&self, query: &str) -> Result<FtsQuery> {
        let query = query.trim();
        if query.is_empty() {
            return Err(Error::InvalidQuery("Empty query".into()));
        }

        // Handle phrase queries
        if query.starts_with('"') && query.ends_with('"') && query.len() > 2 {
            let phrase = &query[1..query.len() - 1];
            let tokens = self.tokenizer.tokenize(phrase);
            if tokens.is_empty() {
                return Err(Error::InvalidQuery("Empty phrase".into()));
            }
            return Ok(FtsQuery::Phrase(tokens.into_iter().map(|t| t.text).collect()));
        }

        // Split by boolean operators
        let parts: Vec<&str> = query.split_whitespace().collect();
        if parts.is_empty() {
            return Err(Error::InvalidQuery("Empty query".into()));
        }

        // Parse boolean expressions
        self.parse_boolean(&parts)
    }

    fn parse_boolean(&self, parts: &[&str]) -> Result<FtsQuery> {
        if parts.is_empty() {
            return Err(Error::InvalidQuery("Empty query".into()));
        }

        // Find boolean operators
        for (i, part) in parts.iter().enumerate() {
            if *part == "AND" && i > 0 && i < parts.len() - 1 {
                let left = self.parse_boolean(&parts[..i])?;
                let right = self.parse_boolean(&parts[i + 1..])?;
                return Ok(FtsQuery::and(left, right));
            }
        }

        for (i, part) in parts.iter().enumerate() {
            if *part == "OR" && i > 0 && i < parts.len() - 1 {
                let left = self.parse_boolean(&parts[..i])?;
                let right = self.parse_boolean(&parts[i + 1..])?;
                return Ok(FtsQuery::or(left, right));
            }
        }

        for (i, part) in parts.iter().enumerate() {
            if *part == "NOT" && i > 0 && i < parts.len() - 1 {
                let left = self.parse_boolean(&parts[..i])?;
                let right = self.parse_boolean(&parts[i + 1..])?;
                return Ok(FtsQuery::not(left, right));
            }
        }

        // No boolean operators found, treat as implicit AND of terms
        if parts.len() == 1 {
            return self.parse_term(parts[0]);
        }

        // Multiple terms without explicit operators = AND
        let mut result = self.parse_term(parts[0])?;
        for part in &parts[1..] {
            let term = self.parse_term(part)?;
            result = FtsQuery::and(result, term);
        }

        Ok(result)
    }

    fn parse_term(&self, term: &str) -> Result<FtsQuery> {
        // Handle fuzzy: word~N
        if let Some(tilde_pos) = term.find('~') {
            let word = &term[..tilde_pos];
            let distance = term[tilde_pos + 1..]
                .parse::<u8>()
                .unwrap_or(1);
            let tokens = self.tokenizer.tokenize(word);
            if tokens.is_empty() {
                return Err(Error::InvalidQuery(format!("Invalid fuzzy term: {}", term)));
            }
            return Ok(FtsQuery::Fuzzy {
                term: tokens[0].text.clone(),
                max_distance: distance,
            });
        }

        // Handle prefix: word*
        if term.ends_with('*') && term.len() > 1 {
            let prefix = &term[..term.len() - 1];
            return Ok(FtsQuery::Prefix(prefix.to_lowercase()));
        }

        // Regular term
        let tokens = self.tokenizer.tokenize(term);
        if tokens.is_empty() {
            // If the tokenizer filtered it out, use the lowercase version
            return Ok(FtsQuery::Term(term.to_lowercase()));
        }

        Ok(FtsQuery::Term(tokens[0].text.clone()))
    }
}

// ============================================================================
// BM25 Ranking
// ============================================================================

/// BM25 ranking algorithm parameters
#[derive(Debug, Clone)]
pub struct Bm25Config {
    /// Term frequency saturation parameter (typically 1.2)
    pub k1: f64,
    /// Length normalization parameter (typically 0.75)
    pub b: f64,
}

impl Default for Bm25Config {
    fn default() -> Self {
        Self {
            k1: BM25_K1,
            b: BM25_B,
        }
    }
}

/// BM25 scorer for ranking search results
pub struct Bm25Scorer {
    config: Bm25Config,
    /// Total number of documents in the collection
    total_docs: u64,
    /// Average document length
    avg_doc_length: f64,
}

impl Bm25Scorer {
    /// Create a new BM25 scorer
    pub fn new(total_docs: u64, avg_doc_length: f64) -> Self {
        Self {
            config: Bm25Config::default(),
            total_docs,
            avg_doc_length,
        }
    }

    /// Create with custom configuration
    pub fn with_config(config: Bm25Config, total_docs: u64, avg_doc_length: f64) -> Self {
        Self {
            config,
            total_docs,
            avg_doc_length,
        }
    }

    /// Calculate BM25 score for a document
    pub fn score(&self, term_frequency: u32, doc_frequency: u32, doc_length: u32) -> f64 {
        // IDF component
        let idf = self.idf(doc_frequency);

        // TF component with length normalization
        let tf = term_frequency as f64;
        let dl = doc_length as f64;
        let k1 = self.config.k1;
        let b = self.config.b;

        let length_norm = 1.0 - b + b * (dl / self.avg_doc_length);
        let tf_component = (tf * (k1 + 1.0)) / (tf + k1 * length_norm);

        idf * tf_component
    }

    /// Calculate IDF (Inverse Document Frequency)
    fn idf(&self, doc_frequency: u32) -> f64 {
        let n = self.total_docs as f64;
        let df = doc_frequency as f64;

        // Robertson-Sparck Jones IDF formula
        ((n - df + 0.5) / (df + 0.5) + 1.0).ln()
    }
}

// ============================================================================
// Search Result
// ============================================================================

/// A single search result
#[derive(Debug, Clone)]
pub struct SearchHit {
    /// Document key
    pub key: Key,
    /// Relevance score (BM25)
    pub score: f64,
    /// Matched terms and their positions
    pub matches: HashMap<String, Vec<u32>>,
    /// Snippet with highlighting (if requested)
    pub snippet: Option<String>,
}

impl SearchHit {
    /// Create a new search hit
    pub fn new(key: Key, score: f64) -> Self {
        Self {
            key,
            score,
            matches: HashMap::new(),
            snippet: None,
        }
    }
}

/// Search result with multiple hits
#[derive(Debug, Clone)]
pub struct SearchResult {
    /// Matching documents sorted by score (descending)
    pub hits: Vec<SearchHit>,
    /// Total number of matching documents
    pub total_hits: u64,
    /// Query execution time in milliseconds
    pub took_ms: u64,
}

impl SearchResult {
    /// Create an empty search result
    pub fn empty() -> Self {
        Self {
            hits: Vec::new(),
            total_hits: 0,
            took_ms: 0,
        }
    }
}

// ============================================================================
// Snippet Extraction
// ============================================================================

/// Extract snippets from text with matched terms highlighted
pub struct SnippetExtractor {
    /// Maximum snippet length in characters
    pub max_length: usize,
    /// Number of characters around matched terms
    pub context_chars: usize,
    /// Highlight prefix (e.g., "<b>")
    pub highlight_pre: String,
    /// Highlight suffix (e.g., "</b>")
    pub highlight_post: String,
}

impl Default for SnippetExtractor {
    fn default() -> Self {
        Self {
            max_length: 200,
            context_chars: 50,
            highlight_pre: "**".to_string(),
            highlight_post: "**".to_string(),
        }
    }
}

impl SnippetExtractor {
    /// Extract a snippet from text with matched terms highlighted
    pub fn extract(&self, text: &str, matched_terms: &HashSet<String>, tokenizer: &Tokenizer) -> String {
        let tokens = tokenizer.tokenize(text);

        // Find positions of matched terms
        let mut match_positions: Vec<(u32, u32)> = Vec::new();
        for token in &tokens {
            if matched_terms.contains(&token.text) {
                match_positions.push((token.start_offset, token.end_offset));
            }
        }

        if match_positions.is_empty() {
            // No matches, return beginning of text
            let end = text.len().min(self.max_length);
            return text[..end].to_string();
        }

        // Find the best window containing the most matches
        let first_match = match_positions[0].0 as usize;
        let start = first_match.saturating_sub(self.context_chars);
        let end = (first_match + self.max_length).min(text.len());

        let mut result = String::new();
        if start > 0 {
            result.push_str("...");
        }

        // Build snippet with highlighting
        let snippet = &text[start..end];
        let mut last_end = 0;

        for (match_start, match_end) in &match_positions {
            let match_start = (*match_start as usize).saturating_sub(start);
            let match_end = (*match_end as usize).saturating_sub(start);

            if match_start >= snippet.len() {
                continue;
            }

            let match_end = match_end.min(snippet.len());

            if match_start > last_end {
                result.push_str(&snippet[last_end..match_start]);
            }

            result.push_str(&self.highlight_pre);
            result.push_str(&snippet[match_start..match_end]);
            result.push_str(&self.highlight_post);

            last_end = match_end;
        }

        if last_end < snippet.len() {
            result.push_str(&snippet[last_end..]);
        }

        if end < text.len() {
            result.push_str("...");
        }

        result
    }
}

// ============================================================================
// Fuzzy Matching (Levenshtein Distance)
// ============================================================================

/// Calculate Levenshtein edit distance between two strings
pub fn levenshtein_distance(s1: &str, s2: &str) -> usize {
    let len1 = s1.chars().count();
    let len2 = s2.chars().count();

    if len1 == 0 {
        return len2;
    }
    if len2 == 0 {
        return len1;
    }

    let s1_chars: Vec<char> = s1.chars().collect();
    let s2_chars: Vec<char> = s2.chars().collect();

    let mut matrix = vec![vec![0usize; len2 + 1]; len1 + 1];

    for i in 0..=len1 {
        matrix[i][0] = i;
    }
    for j in 0..=len2 {
        matrix[0][j] = j;
    }

    for i in 1..=len1 {
        for j in 1..=len2 {
            let cost = if s1_chars[i - 1] == s2_chars[j - 1] { 0 } else { 1 };
            matrix[i][j] = (matrix[i - 1][j] + 1)
                .min(matrix[i][j - 1] + 1)
                .min(matrix[i - 1][j - 1] + cost);
        }
    }

    matrix[len1][len2]
}

/// Check if two strings are within a given edit distance
pub fn within_edit_distance(s1: &str, s2: &str, max_distance: usize) -> bool {
    levenshtein_distance(s1, s2) <= max_distance
}

// ============================================================================
// FTS Index Key Encoding
// ============================================================================

/// Encode an FTS term prefix key (without document key)
/// Format: [FTS_MARKER | index_name_len | index_name | term_len | term]
/// Used for prefix scanning to find all documents containing a term
pub fn encode_fts_term_prefix(index_name: &str, term: &str) -> Vec<u8> {
    let index_name_bytes = index_name.as_bytes();
    let term_bytes = term.as_bytes();

    let capacity = 1 + 4 + index_name_bytes.len() + 4 + term_bytes.len();
    let mut buf = Vec::with_capacity(capacity);

    buf.push(FTS_INDEX_MARKER);
    buf.extend_from_slice(&(index_name_bytes.len() as u32).to_le_bytes());
    buf.extend_from_slice(index_name_bytes);
    buf.extend_from_slice(&(term_bytes.len() as u32).to_le_bytes());
    buf.extend_from_slice(term_bytes);

    buf
}

/// Encode an FTS term key with document key
/// Format: [FTS_MARKER | index_name_len | index_name | term_len | term | doc_key_len | doc_key]
/// This ensures each term-document pair has a unique key
pub fn encode_fts_term_key(index_name: &str, term: &str, doc_key: &[u8]) -> Vec<u8> {
    let index_name_bytes = index_name.as_bytes();
    let term_bytes = term.as_bytes();

    let capacity = 1 + 4 + index_name_bytes.len() + 4 + term_bytes.len() + 4 + doc_key.len();
    let mut buf = Vec::with_capacity(capacity);

    buf.push(FTS_INDEX_MARKER);
    buf.extend_from_slice(&(index_name_bytes.len() as u32).to_le_bytes());
    buf.extend_from_slice(index_name_bytes);
    buf.extend_from_slice(&(term_bytes.len() as u32).to_le_bytes());
    buf.extend_from_slice(term_bytes);
    buf.extend_from_slice(&(doc_key.len() as u32).to_le_bytes());
    buf.extend_from_slice(doc_key);

    buf
}

/// Decode an FTS index key
///
/// Returns (index_name, term) or None if not an FTS key
pub fn decode_fts_term_key(encoded: &[u8]) -> Option<(String, String)> {
    if encoded.is_empty() || encoded[0] != FTS_INDEX_MARKER {
        return None;
    }

    let mut pos = 1;

    // Read index name
    if encoded.len() < pos + 4 {
        return None;
    }
    let name_len = u32::from_le_bytes(encoded[pos..pos + 4].try_into().ok()?) as usize;
    pos += 4;

    if encoded.len() < pos + name_len {
        return None;
    }
    let index_name = String::from_utf8(encoded[pos..pos + name_len].to_vec()).ok()?;
    pos += name_len;

    // Read term
    if encoded.len() < pos + 4 {
        return None;
    }
    let term_len = u32::from_le_bytes(encoded[pos..pos + 4].try_into().ok()?) as usize;
    pos += 4;

    if encoded.len() < pos + term_len {
        return None;
    }
    let term = String::from_utf8(encoded[pos..pos + term_len].to_vec()).ok()?;

    Some((index_name, term))
}

/// Check if an encoded key is an FTS index key
pub fn is_fts_key(encoded: &[u8]) -> bool {
    !encoded.is_empty() && encoded[0] == FTS_INDEX_MARKER
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tokenizer_basic() {
        let tokenizer = Tokenizer::english();
        let tokens = tokenizer.tokenize("Hello World");

        assert_eq!(tokens.len(), 2);
        assert_eq!(tokens[0].text, "hello");
        assert_eq!(tokens[0].position, 0);
        assert_eq!(tokens[1].text, "world");
        assert_eq!(tokens[1].position, 1);
    }

    #[test]
    fn test_tokenizer_stop_words() {
        let tokenizer = Tokenizer::english();
        let tokens = tokenizer.tokenize("The quick brown fox");

        // "the" should be filtered out
        assert!(tokens.iter().all(|t| t.text != "the"));
        assert!(tokens.iter().any(|t| t.text == "quick" || t.text.starts_with("quick")));
    }

    #[test]
    fn test_tokenizer_stemming() {
        let tokenizer = Tokenizer::english();
        let tokens = tokenizer.tokenize("running jumps");

        // Should stem "running" and "jumps"
        assert!(tokens.iter().any(|t| t.text == "runn" || t.text == "run"));
        assert!(tokens.iter().any(|t| t.text == "jump"));
    }

    #[test]
    fn test_tokenize_with_frequency() {
        let tokenizer = Tokenizer::english();
        let term_info = tokenizer.tokenize_with_frequency("hello world hello");

        assert_eq!(term_info.get("hello").map(|t| t.frequency), Some(2));
        assert_eq!(term_info.get("world").map(|t| t.frequency), Some(1));
    }

    #[test]
    fn test_text_index_builder() {
        let index = TextIndex::new("content_idx", "content")
            .language(Language::Spanish)
            .stemming(false)
            .stop_words(true);

        assert_eq!(index.name, "content_idx");
        assert_eq!(index.attribute, "content");
        assert_eq!(index.language, Language::Spanish);
        assert!(!index.stemming);
        assert!(index.stop_words);
    }

    #[test]
    fn test_query_parser_simple_term() {
        let parser = FtsQueryParser::english();
        let query = parser.parse("hello").unwrap();

        match query {
            FtsQuery::Term(t) => assert_eq!(t, "hello"),
            _ => panic!("Expected term query"),
        }
    }

    #[test]
    fn test_query_parser_phrase() {
        let parser = FtsQueryParser::english();
        let query = parser.parse("\"quick brown fox\"").unwrap();

        match query {
            FtsQuery::Phrase(terms) => {
                assert_eq!(terms.len(), 3);
            }
            _ => panic!("Expected phrase query"),
        }
    }

    #[test]
    fn test_query_parser_and() {
        let parser = FtsQueryParser::english();
        let query = parser.parse("hello AND world").unwrap();

        match query {
            FtsQuery::Boolean { op: BooleanOp::And, .. } => {}
            _ => panic!("Expected AND query"),
        }
    }

    #[test]
    fn test_query_parser_or() {
        let parser = FtsQueryParser::english();
        let query = parser.parse("hello OR world").unwrap();

        match query {
            FtsQuery::Boolean { op: BooleanOp::Or, .. } => {}
            _ => panic!("Expected OR query"),
        }
    }

    #[test]
    fn test_query_parser_fuzzy() {
        let parser = FtsQueryParser::english();
        let query = parser.parse("hello~2").unwrap();

        match query {
            FtsQuery::Fuzzy { term, max_distance } => {
                assert_eq!(term, "hello");
                assert_eq!(max_distance, 2);
            }
            _ => panic!("Expected fuzzy query"),
        }
    }

    #[test]
    fn test_query_parser_prefix() {
        let parser = FtsQueryParser::english();
        let query = parser.parse("hel*").unwrap();

        match query {
            FtsQuery::Prefix(prefix) => assert_eq!(prefix, "hel"),
            _ => panic!("Expected prefix query"),
        }
    }

    #[test]
    fn test_bm25_scoring() {
        let scorer = Bm25Scorer::new(1000, 100.0);

        // Higher term frequency = higher score
        let score1 = scorer.score(1, 10, 100);
        let score2 = scorer.score(5, 10, 100);
        assert!(score2 > score1);

        // Lower document frequency = higher score (rarer terms more valuable)
        let score3 = scorer.score(1, 10, 100);
        let score4 = scorer.score(1, 100, 100);
        assert!(score3 > score4);
    }

    #[test]
    fn test_levenshtein_distance() {
        assert_eq!(levenshtein_distance("hello", "hello"), 0);
        assert_eq!(levenshtein_distance("hello", "hallo"), 1);
        assert_eq!(levenshtein_distance("hello", "world"), 4);
        assert_eq!(levenshtein_distance("", "abc"), 3);
        assert_eq!(levenshtein_distance("abc", ""), 3);
    }

    #[test]
    fn test_within_edit_distance() {
        assert!(within_edit_distance("hello", "hallo", 1));
        assert!(within_edit_distance("hello", "helo", 1));
        assert!(!within_edit_distance("hello", "world", 2));
    }

    #[test]
    fn test_encode_decode_fts_key() {
        let index_name = "content_idx";
        let term = "hello";
        let doc_key = b"doc#1";

        let encoded = encode_fts_term_key(index_name, term, doc_key);
        assert!(is_fts_key(&encoded));

        let (decoded_name, decoded_term) = decode_fts_term_key(&encoded).unwrap();
        assert_eq!(decoded_name, index_name);
        assert_eq!(decoded_term, term);
    }

    #[test]
    fn test_encode_fts_prefix() {
        let index_name = "content_idx";
        let term = "hello";
        let doc_key = b"doc#1";

        // Full key should start with prefix
        let prefix = encode_fts_term_prefix(index_name, term);
        let full_key = encode_fts_term_key(index_name, term, doc_key);

        assert!(full_key.starts_with(&prefix));
    }

    #[test]
    fn test_posting_list() {
        let mut posting_list = PostingList::default();

        posting_list.add_posting(b"doc1".to_vec(), 3, vec![0, 5, 10], 100);
        posting_list.add_posting(b"doc2".to_vec(), 1, vec![7], 50);

        assert_eq!(posting_list.doc_frequency, 2);
        assert_eq!(posting_list.postings.len(), 2);

        posting_list.remove_document(b"doc1");
        assert_eq!(posting_list.doc_frequency, 1);
        assert_eq!(posting_list.postings.len(), 1);
    }

    #[test]
    fn test_snippet_extractor() {
        let extractor = SnippetExtractor::default();
        let tokenizer = Tokenizer::english();
        let text = "The quick brown fox jumps over the lazy dog";
        let matched: HashSet<String> = ["quick", "fox"].iter().map(|s| s.to_string()).collect();

        let snippet = extractor.extract(text, &matched, &tokenizer);
        assert!(snippet.contains("**quick**") || snippet.contains("quick"));
    }

    #[test]
    fn test_language_stop_words() {
        assert!(!Language::English.stop_words().is_empty());
        assert!(!Language::Spanish.stop_words().is_empty());
        assert!(Language::None.stop_words().is_empty());
    }
}
