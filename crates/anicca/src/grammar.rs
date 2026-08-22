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
    pub struct Title(
        #[rust_sitter::leaf(pattern = r#"\"([^\"\\]|\\.)*\""#, transform = |v| serde_json::from_str(v).expect("valid JSON title"))]
        pub String,
    );
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct Uid {
        #[rust_sitter::leaf(text = "^")]
        _marker: (),
        pub raw: UidText,
    }
    impl Uid {
        pub fn new(value: String) -> Self {
            Self {
                _marker: (),
                raw: UidText(value),
            }
        }
        pub fn value(&self) -> String {
            self.raw.0.clone()
        }
    }
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct UidText(
        #[rust_sitter::leaf(pattern = r"(?:r|freq|rule)_[0-9A-HJKMNP-TV-Z]{26}", transform = |v| v.to_string())]
        pub String,
    );
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
        #[rust_sitter::leaf(pattern = r"([^\n]|\n[^}])+", transform = |v| v.strip_prefix('\n').unwrap_or(v).to_string())]
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
        #[rust_sitter::leaf(text = "{")]
        _open_description: (),
        pub description: Option<Description>,
        #[rust_sitter::leaf(text = "}")]
        _close_description: (),
        pub uid: Option<Uid>,
    }
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
        #[rust_sitter::leaf(text = "Frequency")]
        _frequency: (),
        pub name: Name,
        #[rust_sitter::leaf(text = "{")]
        _open: (),
        pub fields: Vec<FrequencyField>,
        #[rust_sitter::leaf(text = "}")]
        _close: (),
        pub uid: Option<Uid>,
    }
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
        #[rust_sitter::leaf(text = "Karma")]
        _karma: (),
        pub name: Name,
        #[rust_sitter::leaf(text = "{")]
        _open: (),
        pub rules: Rules,
        #[rust_sitter::leaf(text = "}")]
        _close: (),
    }
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
