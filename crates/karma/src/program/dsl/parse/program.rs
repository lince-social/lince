use std::collections::BTreeMap;

use super::{KarmaDslError, Parser};
use crate::{
    CapabilitySet, InputBinding, InputSource, LocalId, NodeAst, NodeOperation, OutputRef,
    ParameterDefinition, PortContract, ProgramAst, Slug, StateContract, TriggerSource,
};

impl Parser<'_> {
    pub(super) fn parse_program(mut self) -> Result<ProgramAst, KarmaDslError> {
        self.expect_word("karma")?;
        self.expect_word("1")?;
        self.expect_symbol(';')?;
        self.expect_word("program")?;
        let slug = Slug::new(self.take_word("program slug")?).map_err(|error| self.atom(error))?;
        self.expect_symbol('{')?;

        let mut schema = None;
        let mut purpose = None;
        let mut tags = None;
        let mut capabilities = None;
        let mut parameters = BTreeMap::new();
        let mut nodes = BTreeMap::new();
        let mut outputs = BTreeMap::new();

        while !self.consume_symbol('}') {
            let declaration = self.take_word("program declaration")?;
            match declaration {
                "schema" => {
                    let value = self.parse_enum_atom("program schema")?;
                    self.set_once(&mut schema, value, "schema")?;
                }
                "purpose" => {
                    let value = self.take_string("program purpose")?;
                    self.set_once(&mut purpose, value, "purpose")?;
                }
                "tags" => {
                    let value = self.parse_slug_set()?;
                    self.set_once(&mut tags, value, "tags")?;
                }
                "capabilities" => {
                    let value = self.parse_capability_set()?;
                    self.set_once(&mut capabilities, value, "capabilities")?;
                }
                "param" => {
                    let (id, parameter) = self.parse_parameter()?;
                    self.insert_unique(&mut parameters, id, parameter)?;
                    continue;
                }
                "node" => {
                    let (id, node) = self.parse_node()?;
                    self.insert_unique(&mut nodes, id, node)?;
                    continue;
                }
                "output" => {
                    let id = self.parse_local_id("program output id")?;
                    self.expect_symbol('=')?;
                    let source = self.parse_output_ref()?;
                    self.expect_symbol(';')?;
                    self.insert_unique(&mut outputs, id, source)?;
                    continue;
                }
                _ => return Err(self.unexpected(format!("unknown declaration {declaration:?}"))),
            }
            self.expect_symbol(';')?;
        }
        self.finished("trailing input after program")?;
        Ok(ProgramAst {
            schema: schema.ok_or_else(|| self.missing("schema"))?,
            slug,
            purpose: purpose.ok_or_else(|| self.missing("purpose"))?,
            tags: tags.ok_or_else(|| self.missing("tags"))?,
            parameters,
            nodes,
            outputs,
            required_capabilities: capabilities.ok_or_else(|| self.missing("capabilities"))?,
        })
    }

    fn parse_parameter(&mut self) -> Result<(LocalId, ParameterDefinition), KarmaDslError> {
        let id = self.parse_local_id("parameter id")?;
        self.expect_symbol(':')?;
        let value_type = self.parse_type(0)?;
        let mutable = match self.take_word("parameter mutability")? {
            "mutable" => true,
            "fixed" => false,
            value => return Err(self.unexpected(format!("unknown parameter mutability {value:?}"))),
        };
        self.expect_symbol('=')?;
        let default = self.parse_literal(0)?;
        self.expect_symbol(';')?;
        Ok((
            id,
            ParameterDefinition {
                value_type,
                default,
                mutable,
            },
        ))
    }

    fn parse_node(&mut self) -> Result<(LocalId, NodeAst), KarmaDslError> {
        let id = self.parse_local_id("node id")?;
        self.expect_symbol('{')?;
        let mut inputs = BTreeMap::new();
        let mut outputs = BTreeMap::new();
        let mut operation = None;
        while !self.consume_symbol('}') {
            let declaration = self.take_word("node declaration")?;
            match declaration {
                "bind" => {
                    let input_id = self.parse_local_id("input binding id")?;
                    self.expect_symbol(':')?;
                    let expected_type = self.parse_type(0)?;
                    self.expect_symbol('=')?;
                    let source = self.parse_output_ref()?;
                    self.expect_symbol(';')?;
                    self.insert_unique(
                        &mut inputs,
                        input_id,
                        InputBinding {
                            source,
                            expected_type,
                        },
                    )?;
                }
                "port" => {
                    let port_id = self.parse_local_id("output port id")?;
                    self.expect_symbol(':')?;
                    let value_type = self.parse_type(0)?;
                    self.expect_word("sensitivity")?;
                    let sensitivity = self.parse_enum_atom("sensitivity")?;
                    self.expect_word("freshness")?;
                    let freshness = if self.consume_word("none") {
                        None
                    } else {
                        Some(self.parse_duration_constructor("freshness milliseconds")?)
                    };
                    self.expect_symbol(';')?;
                    self.insert_unique(
                        &mut outputs,
                        port_id,
                        PortContract {
                            value_type,
                            sensitivity,
                            freshness,
                        },
                    )?;
                }
                "op" => {
                    let value = self.parse_operation()?;
                    self.set_once(&mut operation, value, "node operation")?;
                }
                _ => {
                    return Err(
                        self.unexpected(format!("unknown node declaration {declaration:?}"))
                    );
                }
            }
        }
        Ok((
            id,
            NodeAst {
                inputs,
                outputs,
                operation: operation.ok_or_else(|| self.missing("node operation"))?,
            },
        ))
    }

    fn parse_operation(&mut self) -> Result<NodeOperation, KarmaDslError> {
        let kind = self.take_word("node operation kind")?;
        let operation = match kind {
            "trigger" => {
                self.expect_symbol('(')?;
                let source = self.parse_trigger_source()?;
                self.comma()?;
                let output = self.parse_local_id("trigger output")?;
                NodeOperation::Trigger { source, output }
            }
            "input" => {
                self.expect_symbol('(')?;
                let source = self.parse_input_source()?;
                self.comma()?;
                let output = self.parse_local_id("input output")?;
                NodeOperation::Input { source, output }
            }
            "derive" => {
                self.expect_symbol('{')?;
                let mut expressions = BTreeMap::new();
                while !self.consume_symbol('}') {
                    self.expect_word("expr")?;
                    let output = self.parse_local_id("derive output")?;
                    self.expect_symbol('=')?;
                    let expression = self.parse_expression(0)?;
                    self.expect_symbol(';')?;
                    self.insert_unique(&mut expressions, output, expression)?;
                }
                return Ok(NodeOperation::Derive { expressions });
            }
            "delay" => {
                self.expect_symbol('(')?;
                let input = self.parse_local_id("delay input")?;
                self.comma()?;
                let output = self.parse_local_id("delay output")?;
                self.comma()?;
                let initial = self.parse_literal(0)?;
                self.comma()?;
                let state = self.parse_state_contract()?;
                NodeOperation::Delay {
                    input,
                    output,
                    initial,
                    state,
                }
            }
            "threshold" => {
                self.expect_symbol('(')?;
                let input = self.parse_local_id("threshold input")?;
                self.comma()?;
                let active = self.parse_local_id("threshold active output")?;
                self.comma()?;
                let entered = self.parse_local_id("threshold entered output")?;
                self.comma()?;
                let left = self.parse_local_id("threshold left output")?;
                self.comma()?;
                let direction = self.parse_enum_atom("threshold direction")?;
                self.comma()?;
                let enter = self.parse_literal(0)?;
                self.comma()?;
                let exit = self.parse_literal(0)?;
                self.comma()?;
                let initial_active = self.parse_bool()?;
                self.comma()?;
                let state = self.parse_state_contract()?;
                NodeOperation::Threshold {
                    input,
                    active,
                    entered,
                    left,
                    direction,
                    enter,
                    exit,
                    initial_active,
                    state,
                }
            }
            "debounce" => {
                self.expect_symbol('(')?;
                let input = self.parse_local_id("debounce input")?;
                self.comma()?;
                let stable = self.parse_local_id("debounce stable output")?;
                self.comma()?;
                let entered = self.parse_local_id("debounce entered output")?;
                self.comma()?;
                let left = self.parse_local_id("debounce left output")?;
                self.comma()?;
                let for_at_least = self.parse_duration_constructor("debounce duration")?;
                self.comma()?;
                let initial = self.parse_bool()?;
                self.comma()?;
                let state = self.parse_state_contract()?;
                NodeOperation::Debounce {
                    input,
                    stable,
                    entered,
                    left,
                    for_at_least,
                    initial,
                    state,
                }
            }
            "cooldown" => {
                self.expect_symbol('(')?;
                let input = self.parse_local_id("cooldown input")?;
                self.comma()?;
                let allowed = self.parse_local_id("cooldown allowed output")?;
                self.comma()?;
                let cooldown = self.parse_duration_constructor("cooldown duration")?;
                self.comma()?;
                let state = self.parse_state_contract()?;
                NodeOperation::Cooldown {
                    input,
                    allowed,
                    cooldown,
                    state,
                }
            }
            "rate-limit" => {
                self.expect_symbol('(')?;
                let input = self.parse_local_id("rate-limit input")?;
                self.comma()?;
                let allowed = self.parse_local_id("rate-limit allowed output")?;
                self.comma()?;
                let max = self.parse_atom("rate-limit maximum")?;
                self.comma()?;
                let window = self.parse_duration_constructor("rate-limit window")?;
                self.comma()?;
                let state = self.parse_state_contract()?;
                NodeOperation::RateLimit {
                    input,
                    allowed,
                    max,
                    window,
                    state,
                }
            }
            "route-candidate" => {
                self.expect_symbol('(')?;
                let condition = self.parse_local_id("candidate condition")?;
                self.comma()?;
                let output = self.parse_local_id("candidate output")?;
                self.comma()?;
                let route = self.parse_enum_atom("candidate route")?;
                self.comma()?;
                let template = Slug::new(self.take_word("candidate template")?)
                    .map_err(|error| self.atom(error))?;
                self.comma()?;
                let fields = self.parse_candidate_input_fields()?;
                NodeOperation::RouteCandidate {
                    condition,
                    output,
                    route,
                    template,
                    fields,
                }
            }
            _ => return Err(self.unexpected(format!("unknown node operation {kind:?}"))),
        };
        self.expect_symbol(')')?;
        self.expect_symbol(';')?;
        Ok(operation)
    }

    fn parse_candidate_input_fields(
        &mut self,
    ) -> Result<BTreeMap<LocalId, LocalId>, KarmaDslError> {
        self.parse_id_map("candidate field", '=', |parser| {
            parser.parse_local_id("candidate field input")
        })
    }

    fn parse_state_contract(&mut self) -> Result<StateContract, KarmaDslError> {
        self.expect_word("state")?;
        self.expect_symbol('(')?;
        let persistence = self.parse_enum_atom("state persistence")?;
        self.comma()?;
        let reset = self.parse_enum_atom("state reset policy")?;
        self.comma()?;
        let late_event = self.parse_enum_atom("late event policy")?;
        self.comma()?;
        let migration = self.parse_enum_atom("state migration policy")?;
        self.comma()?;
        let simulation = self.parse_enum_atom("simulation state policy")?;
        self.expect_symbol(')')?;
        Ok(StateContract {
            persistence,
            reset,
            late_event,
            migration,
            simulation,
        })
    }

    fn parse_trigger_source(&mut self) -> Result<TriggerSource, KarmaDslError> {
        let kind = self.take_word("trigger source")?;
        match kind {
            "manual" => Ok(TriggerSource::Manual),
            "sync" => Ok(TriggerSource::Sync),
            "fact" => {
                self.expect_symbol('(')?;
                let record = self.parse_optional_reference()?;
                self.comma()?;
                let concept = self.parse_optional_reference()?;
                self.expect_symbol(')')?;
                Ok(TriggerSource::Fact { record, concept })
            }
            "frequency" => Ok(TriggerSource::Frequency {
                frequency: self.parse_single_reference_argument()?,
            }),
            "signal" => Ok(TriggerSource::Signal {
                signal: self.parse_single_reference_argument()?,
            }),
            "decision" => Ok(TriggerSource::Decision {
                decision: self.parse_single_reference_argument()?,
            }),
            "receipt" => Ok(TriggerSource::Receipt {
                receipt: self.parse_single_reference_argument()?,
            }),
            _ => Err(self.unexpected(format!("unknown trigger source {kind:?}"))),
        }
    }

    fn parse_input_source(&mut self) -> Result<InputSource, KarmaDslError> {
        let kind = self.take_word("input source")?;
        match kind {
            "parameter" => Ok(InputSource::Parameter {
                parameter: self.parse_single_local_id_argument("parameter")?,
            }),
            "record-quantity" => Ok(InputSource::RecordQuantity {
                record: self.parse_single_reference_argument()?,
            }),
            "saved-protein" => Ok(InputSource::SavedProtein {
                view: self.parse_single_reference_argument()?,
            }),
            "signal" => Ok(InputSource::Signal {
                signal: self.parse_single_reference_argument()?,
            }),
            "captured-fact" => Ok(InputSource::CapturedFact {
                fact: self.parse_single_reference_argument()?,
            }),
            _ => Err(self.unexpected(format!("unknown input source {kind:?}"))),
        }
    }

    fn parse_output_ref(&mut self) -> Result<OutputRef, KarmaDslError> {
        self.expect_word("source")?;
        self.expect_symbol('(')?;
        let node = self.parse_local_id("source node")?;
        self.comma()?;
        let port = self.parse_local_id("source port")?;
        self.expect_symbol(')')?;
        Ok(OutputRef { node, port })
    }

    fn parse_capability_set(&mut self) -> Result<CapabilitySet, KarmaDslError> {
        self.parse_set("capability", |parser| parser.parse_enum_atom("capability"))
            .map(CapabilitySet::new)
    }
}
