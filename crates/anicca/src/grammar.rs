#[rust_sitter::grammar("anicca")]
pub mod grammar {
    #[rust_sitter::extra]
    #[allow(dead_code)]
    struct Whitespace {
        #[rust_sitter::leaf(pattern = r"\s")]
        _whitespace: (),
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct Name(
        #[rust_sitter::leaf(pattern = r"[A-Za-z][A-Za-z0-9.-]*", transform = |v| v.to_string())]
        pub String,
    );
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct Title {
        pub first: TitleWord,
        pub rest: Vec<TitleWord>,
    }
    impl Title {
        pub fn value(&self) -> String {
            std::iter::once(&self.first)
                .chain(&self.rest)
                .map(TitleWord::value)
                .collect::<Vec<_>>()
                .join(" ")
        }
        pub fn set(&mut self, value: &str) {
            let mut words = value.split_whitespace().map(TitleWord::ordinary);
            self.first = words.next().unwrap_or_else(|| TitleWord::ordinary(""));
            self.rest = words.collect();
        }
    }
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum TitleWord {
        Rules(#[rust_sitter::leaf(text = "Rules", transform = |v| v.to_string())] String),
        Rule(#[rust_sitter::leaf(text = "Rule", transform = |v| v.to_string())] String),
        Title(#[rust_sitter::leaf(text = "title", transform = |v| v.to_string())] String),
        Quantity(#[rust_sitter::leaf(text = "quantity", transform = |v| v.to_string())] String),
        Every(#[rust_sitter::leaf(text = "every", transform = |v| v.to_string())] String),
        Timezone(#[rust_sitter::leaf(text = "timezone", transform = |v| v.to_string())] String),
        NextAt(#[rust_sitter::leaf(text = "next_at", transform = |v| v.to_string())] String),
        Frequency(#[rust_sitter::leaf(text = "frequency", transform = |v| v.to_string())] String),
        Record(#[rust_sitter::leaf(text = "record", transform = |v| v.to_string())] String),
        Condition(#[rust_sitter::leaf(text = "condition", transform = |v| v.to_string())] String),
        Gate(#[rust_sitter::leaf(text = "gate", transform = |v| v.to_string())] String),
        Carry(#[rust_sitter::leaf(text = "carry", transform = |v| v.to_string())] String),
        Consequences(
            #[rust_sitter::leaf(text = "consequences", transform = |v| v.to_string())] String,
        ),
        Note(#[rust_sitter::leaf(text = "note", transform = |v| v.to_string())] String),
        Is(#[rust_sitter::leaf(text = "is", transform = |v| v.to_string())] String),
        Ordinary(
            #[rust_sitter::word]
            #[rust_sitter::leaf(pattern = r"[^(){}\s^]+", transform = |v| v.to_string())]
            String,
        ),
    }
    impl TitleWord {
        fn ordinary(value: &str) -> Self {
            Self::Ordinary(value.to_string())
        }
        fn value(&self) -> &str {
            match self {
                Self::Rules(value)
                | Self::Rule(value)
                | Self::Title(value)
                | Self::Quantity(value)
                | Self::Every(value)
                | Self::Timezone(value)
                | Self::NextAt(value)
                | Self::Frequency(value)
                | Self::Record(value)
                | Self::Condition(value)
                | Self::Gate(value)
                | Self::Carry(value)
                | Self::Consequences(value)
                | Self::Note(value)
                | Self::Is(value)
                | Self::Ordinary(value) => value,
            }
        }
    }
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct Uid(
        #[rust_sitter::leaf(pattern = r"\^(?:r|freq|rule)_[0-9A-HJKMNP-TV-Z]{26}", transform = |v| v.strip_prefix('^').expect("uid marker").to_string())]
        pub String,
    );
    impl Uid {
        pub fn new(value: String) -> Self {
            Self(value)
        }
        pub fn value(&self) -> String {
            self.0.clone()
        }
    }
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct Text(
        #[rust_sitter::leaf(pattern = r#"\"([^\"\\]|\\.)*\""#, transform = |v| serde_json::from_str(v).expect("valid JSON string"))]
        pub String,
    );
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct Body(
        #[rust_sitter::leaf(pattern = r#"\"\"\"([^\"]|\"[^\"]|\"\"[^\"])*\"\"\""#, transform = |v| v[3..v.len() - 3].to_string())]
        pub String,
    );
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct Description(
        #[rust_sitter::leaf(pattern = r#"\n(?:[^}\n][^\n]*|}[ \t]+\"[^\n]*)?(?:\n(?:[^}\n][^\n]*|}[ \t]+\"[^\n]*)?)*"#, transform = |v| v.strip_prefix('\n').unwrap_or(v).to_string())]
        pub String,
    );
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct Number(
        #[rust_sitter::leaf(pattern = r"-?[0-9]+(?:\.[0-9]+)?", transform = |v| v.to_string())]
        pub String,
    );
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct Instant(
        #[rust_sitter::leaf(pattern = r"[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}(?:\.[0-9]+)?(?:Z|[+-][0-9]{2}:[0-9]{2})", transform = |v| v.to_string())]
        pub String,
    );
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct Comment(
        #[rust_sitter::leaf(pattern = r"//[^\n]*", transform = |v| v.to_string())]
        pub String,
    );

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct Reference {
        #[rust_sitter::leaf(text = "@")]
        _at: (),
        pub slug: Name,
    }
    impl Reference {
        pub fn new(slug: String) -> Self {
            Self {
                _at: (),
                slug: Name(slug),
            }
        }
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    #[rust_sitter::language]
    pub struct Document {
        pub declarations: Vec<Declaration>,
    }
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum Declaration {
        Frequency(Frequency),
        Karma(Karma),
        Record(Record),
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct Record {
        pub title: Title,
        #[rust_sitter::leaf(text = "(")]
        _open_header: (),
        pub header: RecordHeader,
        #[rust_sitter::leaf(text = ")")]
        _close_header: (),
        pub opening: RecordOpening,
        pub description: Option<Description>,
        pub closing: RecordClosing,
    }
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum RecordOpening {
        Identified(IdentifiedRecordOpening),
        Unidentified(UnidentifiedRecordOpening),
    }
    impl RecordOpening {
        pub fn uid(&self) -> Option<&str> {
            match self {
                Self::Identified(value) => Some(&value.0),
                Self::Unidentified(_) => None,
            }
        }
        pub fn identified(uid: String) -> Self {
            Self::Identified(IdentifiedRecordOpening(uid))
        }
    }
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct IdentifiedRecordOpening(
        #[rust_sitter::leaf(pattern = r"\{[ \t]+r_[0-9A-HJKMNP-TV-Z]{26}", transform = |v| v.split_ascii_whitespace().last().expect("opening uid").to_string())]
        pub String,
    );
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct UnidentifiedRecordOpening(#[rust_sitter::leaf(text = "{")] ());
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum RecordClosing {
        Identified(IdentifiedRecordClosing),
        Unidentified(UnidentifiedRecordClosing),
    }
    impl RecordClosing {
        pub fn uid(&self) -> Option<&str> {
            match self {
                Self::Identified(value) => Some(&value.0),
                Self::Unidentified(_) => None,
            }
        }
        pub fn identified(uid: String) -> Self {
            Self::Identified(IdentifiedRecordClosing(uid))
        }
    }
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct IdentifiedRecordClosing(
        #[rust_sitter::leaf(pattern = r"\}[ \t]+r_[0-9A-HJKMNP-TV-Z]{26}", transform = |v| v.split_ascii_whitespace().last().expect("closing uid").to_string())]
        pub String,
    );
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct UnidentifiedRecordClosing(#[rust_sitter::leaf(text = "}")] ());
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct RecordHeader {
        pub subject: RecordSubject,
        pub rest: Vec<RecordHeaderRest>,
    }
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum RecordSubject {
        Slugged(SluggedSubject),
        Anonymous(Quantity),
    }
    impl RecordSubject {
        pub fn slug(&self) -> Option<&str> {
            match self {
                Self::Slugged(v) => Some(&v.slug.0),
                Self::Anonymous(_) => None,
            }
        }
        pub fn quantity(&self) -> &Quantity {
            match self {
                Self::Slugged(v) => &v.quantity,
                Self::Anonymous(v) => v,
            }
        }
        pub fn quantity_mut(&mut self) -> &mut Quantity {
            match self {
                Self::Slugged(v) => &mut v.quantity,
                Self::Anonymous(v) => v,
            }
        }
    }
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct SluggedSubject {
        #[rust_sitter::leaf(text = "@")]
        _at: (),
        pub slug: Name,
        #[rust_sitter::leaf(text = ":")]
        _colon: (),
        pub quantity: Quantity,
    }
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct Quantity {
        pub value: Number,
        pub unit: Option<Unit>,
    }
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct Unit {
        #[rust_sitter::leaf(text = "@")]
        _at: (),
        pub name: Name,
    }
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct RecordHeaderRest {
        #[rust_sitter::leaf(text = ",")]
        _comma: (),
        pub comment: Option<Comment>,
        pub field: RecordField,
    }
    impl RecordHeaderRest {
        pub fn new(field: RecordField) -> Self {
            Self {
                _comma: (),
                comment: None,
                field,
            }
        }
    }
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum RecordField {
        Identity(IdentityField),
        Assertion(AssertionField),
    }
    impl RecordField {
        pub fn identity(predicate: String) -> Self {
            Self::Identity(IdentityField {
                _is: (),
                _hash: (),
                predicate: Name(predicate),
            })
        }
        pub fn assertion(predicate: String) -> Self {
            Self::Assertion(AssertionField {
                _hash: (),
                predicate: Name(predicate),
                tail: None,
            })
        }
        pub fn link(
            predicate: String,
            slug: String,
            quantity: Option<String>,
            unit: Option<String>,
        ) -> Self {
            Self::Assertion(AssertionField {
                _hash: (),
                predicate: Name(predicate),
                tail: Some(AssertionTail::Link(LinkTail {
                    target: Reference::new(slug),
                    amount: quantity.map(|value| AssertionQuantity {
                        _colon: (),
                        quantity: Quantity {
                            value: Number(value),
                            unit: unit.map(|name| Unit {
                                _at: (),
                                name: Name(name),
                            }),
                        },
                    }),
                })),
            })
        }
        pub fn amount(predicate: String, quantity: String, unit: Option<String>) -> Self {
            Self::Assertion(AssertionField {
                _hash: (),
                predicate: Name(predicate),
                tail: Some(AssertionTail::Amount(AmountTail {
                    _colon: (),
                    quantity: Quantity {
                        value: Number(quantity),
                        unit: unit.map(|name| Unit {
                            _at: (),
                            name: Name(name),
                        }),
                    },
                })),
            })
        }
    }
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct IdentityField {
        #[rust_sitter::leaf(text = "is")]
        _is: (),
        #[rust_sitter::leaf(text = "#")]
        _hash: (),
        pub predicate: Name,
    }
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct AssertionField {
        #[rust_sitter::leaf(text = "#")]
        _hash: (),
        pub predicate: Name,
        pub tail: Option<AssertionTail>,
    }
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum AssertionTail {
        Link(LinkTail),
        Amount(AmountTail),
    }
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct LinkTail {
        pub target: Reference,
        pub amount: Option<AssertionQuantity>,
    }
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct AssertionQuantity {
        #[rust_sitter::leaf(text = ":")]
        _colon: (),
        pub quantity: Quantity,
    }
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct AmountTail {
        #[rust_sitter::leaf(text = ":")]
        _colon: (),
        pub quantity: Quantity,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct Frequency {
        pub opening: FrequencyOpening,
        pub fields: Vec<FrequencyField>,
        #[rust_sitter::leaf(text = "}")]
        _close: (),
        pub uid: Option<Uid>,
    }
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct FrequencyOpening(
        #[rust_sitter::leaf(pattern = r"Frequency[ \t]+[A-Za-z][A-Za-z0-9.-]*[ \t]*\{", transform = |v| v.trim_end_matches('{').split_ascii_whitespace().nth(1).expect("Frequency name").to_string())]
        pub String,
    );
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum FrequencyField {
        Title(TitleField),
        Quantity(QuantityField),
        Every(EveryField),
        Timezone(TimezoneField),
        NextAt(NextAtField),
    }
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct TitleField {
        #[rust_sitter::leaf(text = "title")]
        _keyword: (),
        pub value: Text,
    }
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct QuantityField {
        #[rust_sitter::leaf(text = "quantity")]
        _keyword: (),
        pub value: Number,
        pub unit: Option<Unit>,
    }
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct EveryField {
        #[rust_sitter::leaf(text = "every")]
        _keyword: (),
        pub count: Number,
        pub unit: Name,
    }
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct TimezoneField {
        #[rust_sitter::leaf(text = "timezone")]
        _keyword: (),
        pub value: Text,
    }
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct NextAtField {
        #[rust_sitter::leaf(text = "next_at")]
        _keyword: (),
        pub value: Instant,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct Karma {
        pub opening: KarmaOpening,
        pub rules: Rules,
        #[rust_sitter::leaf(text = "}")]
        _close: (),
    }
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct KarmaOpening(
        #[rust_sitter::leaf(pattern = r"Karma[ \t]+[A-Za-z][A-Za-z0-9.-]*[ \t]*\{", transform = |v| v.trim_end_matches('{').split_ascii_whitespace().nth(1).expect("Karma name").to_string())]
        pub String,
    );
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct Rules {
        #[rust_sitter::leaf(text = "Rules")]
        _rules: (),
        #[rust_sitter::leaf(text = "{")]
        _open: (),
        pub rules: Vec<Rule>,
        #[rust_sitter::leaf(text = "}")]
        _close: (),
    }
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct Rule {
        #[rust_sitter::leaf(text = "Rule")]
        _rule: (),
        pub name: Name,
        #[rust_sitter::leaf(text = "{")]
        _open: (),
        pub fields: Vec<RuleField>,
        #[rust_sitter::leaf(text = "}")]
        _close: (),
        pub uid: Option<Uid>,
    }
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum RuleField {
        Quantity(QuantityField),
        Frequency(FrequencyReferenceField),
        Record(RecordReferenceField),
        Condition(ConditionField),
        Gate(GateField),
        Carry(CarryField),
        Consequences(ConsequencesField),
        Note(NoteField),
    }
    impl RuleField {
        pub fn quantity(value: i64) -> Self {
            Self::Quantity(QuantityField {
                _keyword: (),
                value: Number(value.to_string()),
                unit: None,
            })
        }
        pub fn frequency(slug: String) -> Self {
            Self::Frequency(FrequencyReferenceField {
                _keyword: (),
                value: Reference::new(slug),
            })
        }
        pub fn record(slug: String) -> Self {
            Self::Record(RecordReferenceField {
                _keyword: (),
                value: Reference::new(slug),
            })
        }
        pub fn condition(value: String) -> Self {
            Self::Condition(ConditionField {
                _keyword: (),
                value: Body(value),
            })
        }
        pub fn gate(value: String) -> Self {
            Self::Gate(GateField {
                _keyword: (),
                value: Name(value),
            })
        }
        pub fn carry(value: String) -> Self {
            Self::Carry(CarryField {
                _keyword: (),
                value: Name(value),
            })
        }
        pub fn consequences(value: String) -> Self {
            Self::Consequences(ConsequencesField {
                _keyword: (),
                value: Body(value),
            })
        }
        pub fn note(value: String) -> Self {
            Self::Note(NoteField {
                _keyword: (),
                value: Text(value),
            })
        }
    }
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct FrequencyReferenceField {
        #[rust_sitter::leaf(text = "frequency")]
        _keyword: (),
        pub value: Reference,
    }
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct RecordReferenceField {
        #[rust_sitter::leaf(text = "record")]
        _keyword: (),
        pub value: Reference,
    }
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct ConditionField {
        #[rust_sitter::leaf(text = "condition")]
        _keyword: (),
        pub value: Body,
    }
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct GateField {
        #[rust_sitter::leaf(text = "gate")]
        _keyword: (),
        pub value: Name,
    }
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct CarryField {
        #[rust_sitter::leaf(text = "carry")]
        _keyword: (),
        pub value: Name,
    }
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct ConsequencesField {
        #[rust_sitter::leaf(text = "consequences")]
        _keyword: (),
        pub value: Body,
    }
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct NoteField {
        #[rust_sitter::leaf(text = "note")]
        _keyword: (),
        pub value: Text,
    }
}
