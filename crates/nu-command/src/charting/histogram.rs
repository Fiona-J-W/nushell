use super::hashable_value::HashableValue;
use itertools::Itertools;
use nu_engine::command_prelude::*;

use std::collections::HashMap;

#[derive(Clone)]
pub struct Histogram;


impl Command for Histogram {
	fn name(&self) -> &str {
		"histogram"
	}

	fn signature(&self) -> Signature {
		Signature::build("histogram")
			.input_output_types(vec![(Type::List(Box::new(Type::Any)), Type::table())])
			.optional(
				"column-name",
				SyntaxShape::String,
				"Column name to calc frequency, no need to provide if input is a list.",
			)
			.category(Category::Chart)
	}

	fn description(&self) -> &str {
		"Creates a new table with a histogram based on the column name passed in."
	}

	fn examples(&self) -> Vec<Example<'_>> {
		vec![
			Example {
				description: "Compute a histogram of file types",
				example: "ls | histogram type",
				result: None,
			},
			Example {
				description: "Compute a histogram for the types of files, with frequency column \
							  named freq",
				example: "ls | histogram type freq",
				result: None,
			},
			Example {
				// TODO: update
				description: "Compute a histogram for a list of numbers",
				example: "[1 2 1] | histogram",
				result: Some(Value::test_list(vec![
					Value::test_record(record! {
						"value" =>	  Value::test_int(1),
						"count" =>	  Value::test_int(2),
						"quantile" =>   Value::test_float(0.6666666666666666),
						"percentage" => Value::test_string("66.67%"),
						"frequency" =>  Value::test_string("██████████████████████████████████████████████████████████████████▋"),
					}),
					Value::test_record(record! {
						"value" =>	  Value::test_int(2),
						"count" =>	  Value::test_int(1),
						"quantile" =>   Value::test_float(0.3333333333333333),
						"percentage" => Value::test_string("33.33%"),
						"frequency" =>  Value::test_string("█████████████████████████████████▍"),
					}),
				])),
			},
			Example {
				description: "Compute a histogram for a list of numbers, and percentage is based \
							  on the maximum value",
				example: "[1 2 3 1 1 1 2 2 1 1] | histogram --percentage-type relative",
				result: None,
			},
		]
	}

	fn run(
		&self,
		engine_state: &EngineState,
		stack: &mut Stack,
		call: &Call,
		input: PipelineData,
	) -> Result<PipelineData, ShellError> {
		let column_name: Option<Spanned<String>> = call.opt(engine_state, stack, 0)?;

		let head_span = call.head;
		let data_as_value = input.into_value(head_span)?;
		let list_span = data_as_value.span();
		let values = data_as_value.into_list()?;
		let mut inputs = vec![];
		// convert from inputs to hashable values.
		match column_name {
			None => {
				// some invalid input scenario needs to handle:
				// Expect input is a list of hashable value, if one value is not hashable, throw out error.
				for v in values {
					match v {
						// Propagate existing errors.
						Value::Error { error, .. } => return Err(*error),
						_ => {
							let t = v.get_type();
							let span = v.span();
							inputs.push(HashableValue::from_value(v, head_span).map_err(|_| {
								ShellError::UnsupportedInput {
									msg: "Since column-name was not provided, only lists of hashable \
										  values are supported."
										.to_string(),
									input: format!("input type: {t:?}"),
									msg_span: head_span,
									input_span: span,
								}
							})?)
						}
					}
				}
			}
			Some(ref col) => {
				// some invalid input scenario needs to handle:
				// * item in `input` is not a record, just skip it.
				// * a record doesn't contain specific column, just skip it.
				// * all records don't contain specific column, throw out error, indicate at least one row should contains specific column.
				// * a record contain a value which can't be hashed, skip it.
				let col_name = &col.item;
				for v in values {
					match v {
						// parse record, and fill valid value to actual input.
						Value::Record { val, .. } => {
							if let Some(v) = val.get(col_name)
								&& let Ok(v) = HashableValue::from_value(v.clone(), head_span)
							{
								inputs.push(v);
							}
						}
						// Propagate existing errors.
						Value::Error { error, .. } => return Err(*error),
						_ => continue,
					}
				}

				if inputs.is_empty() {
					return Err(ShellError::CantFindColumn {
						col_name: col_name.clone(),
						span: Some(head_span),
						src_span: list_span,
					});
				}
			}
		}

		let value_column_name = column_name
			.map(|x| x.item)
			.unwrap_or_else(|| "value".to_string());
		Ok(histogram_impl(
			inputs,
			&value_column_name,
			head_span,
		))
	}
}

fn histogram_impl(
	inputs: Vec<HashableValue>,
	value_column_name: &str,
	span: Span,
) -> PipelineData {
	// here we can make sure that inputs is not empty, and every elements
	// is a simple val and ok to make count.
	let mut counter = HashMap::new();
	let mut max_cnt = 0;
	let total_cnt = inputs.len();
	for i in inputs {
		let new_cnt = *counter.get(&i).unwrap_or(&0) + 1;
		counter.insert(i, new_cnt);
		if new_cnt > max_cnt {
			max_cnt = new_cnt;
		}
	}
	let mut result = vec![];
	for (val, count) in counter.into_iter().sorted() {
		let normalized = count as f64 / total_cnt as f64;
		let relative = count as f64 / max_cnt as f64;

		result.push((
			count, // attach count first for easily sorting.
			Value::record(
				record! {
					value_column_name => val.into_value(),
					"count" => Value::int(count, span),
					"total-share" => Value::float(normalized, span),
					"relative-share" => Value::float(relative, span),
				},
				span,
			),
		));
	}
	result.sort_by(|a, b| b.0.cmp(&a.0));
	Value::list(result.into_iter().map(|x| x.1).collect(), span).into_pipeline_data()
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_examples() {
		use crate::test_examples;

		test_examples(Histogram)
	}
}
