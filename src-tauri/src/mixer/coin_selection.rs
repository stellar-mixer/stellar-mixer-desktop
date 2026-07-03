use anyhow::{bail, Result};

use crate::models::NoteSecret;

pub const MAX_WITHDRAW_INPUTS: usize = 8;

#[derive(Debug, Clone)]
pub struct CoinSelection {
    pub notes: Vec<NoteSecret>,
    pub input_sum: u128,
    pub change: u128,
}

pub fn total_unspent_balance(notes: &[NoteSecret]) -> u128 {
    notes
        .iter()
        .filter(|note| !note.spent && note.value > 0)
        .map(|note| note.value)
        .sum()
}

pub fn total_spendable_balance(notes: &[NoteSecret]) -> u128 {
    notes
        .iter()
        .filter(|note| !note.spent && note.value > 0 && note.leaf_index.is_some())
        .map(|note| note.value)
        .sum()
}

pub fn unspent_note_count(notes: &[NoteSecret]) -> usize {
    notes
        .iter()
        .filter(|note| !note.spent && note.value > 0)
        .count()
}

pub fn spendable_note_count(notes: &[NoteSecret]) -> usize {
    notes
        .iter()
        .filter(|note| !note.spent && note.value > 0 && note.leaf_index.is_some())
        .count()
}

#[allow(dead_code)]
pub fn select_single_note_for_withdraw(
    notes: &[NoteSecret],
    amount: u128,
) -> Result<CoinSelection> {
    if amount == 0 {
        bail!("withdraw amount must be greater than 0 XLM");
    }

    let total = total_unspent_balance(notes);
    let spendable = total_spendable_balance(notes);

    if total < amount {
        bail!(
            "insufficient private mixer balance: requested {}, available {}",
            format_xlm(amount),
            format_xlm(total)
        );
    }

    if spendable < amount {
        bail!(
            "private mixer balance is enough, but not enough indexed/spendable notes yet: requested {}, spendable {}",
            format_xlm(amount),
            format_xlm(spendable)
        );
    }

    let selected = notes
        .iter()
        .filter(|note| !note.spent && note.value > 0 && note.leaf_index.is_some() && note.value >= amount)
        .min_by(|a, b| {
            a.value
                .cmp(&b.value)
                .then_with(|| a.created_at.cmp(&b.created_at))
                .then_with(|| a.id.cmp(&b.id))
        })
        .cloned()
        .ok_or_else(|| {
            anyhow::anyhow!(
                "no single spendable note can cover {}; withdraw currently supports only 1 input note",
                format_xlm(amount)
            )
        })?;

    Ok(CoinSelection {
        input_sum: selected.value,
        change: selected.value - amount,
        notes: vec![selected],
    })
}

pub fn select_notes_for_transfer(notes: &[NoteSecret], amount: u128) -> Result<CoinSelection> {
    select_notes_for_amount(notes, amount)
}

pub fn select_notes_for_withdraw(notes: &[NoteSecret], amount: u128) -> Result<CoinSelection> {
    select_notes_for_amount(notes, amount)
}

fn select_notes_for_amount(notes: &[NoteSecret], amount: u128) -> Result<CoinSelection> {
    if amount == 0 {
        bail!("amount must be greater than 0 XLM");
    }

    let total_unspent = total_unspent_balance(notes);
    let total_spendable = total_spendable_balance(notes);

    if total_unspent < amount {
        bail!(
            "insufficient private mixer balance: requested {}, available {}",
            format_xlm(amount),
            format_xlm(total_unspent)
        );
    }

    if total_spendable < amount {
        bail!(
            "private mixer balance is enough, but not enough indexed/spendable notes yet: requested {}, spendable {}, pending {}",
            format_xlm(amount),
            format_xlm(total_spendable),
            format_xlm(total_unspent.saturating_sub(total_spendable))
        );
    }

    let mut eligible: Vec<NoteSecret> = notes
        .iter()
        .filter(|note| !note.spent && note.value > 0 && note.leaf_index.is_some())
        .cloned()
        .collect();

    eligible.sort_by(|a, b| {
        b.value
            .cmp(&a.value)
            .then_with(|| a.created_at.cmp(&b.created_at))
            .then_with(|| a.id.cmp(&b.id))
    });

    let mut chosen: Vec<usize> = Vec::new();
    let mut best: Option<CoinSelection> = None;
    let mut visited = 0usize;

    dfs_select(
        &eligible,
        amount,
        0,
        0,
        &mut chosen,
        &mut best,
        &mut visited,
    );

    if let Some(best) = best {
        return Ok(best);
    }

    let min_inputs = minimum_inputs_with_largest_first(&eligible, amount);

    if min_inputs > MAX_WITHDRAW_INPUTS {
        bail!(
            "no suitable note set: this amount would require at least {} input notes, but max supported inputs is {}",
            min_inputs,
            MAX_WITHDRAW_INPUTS
        );
    }

    bail!(
        "no suitable note set found within {} inputs for requested amount {}",
        MAX_WITHDRAW_INPUTS,
        format_xlm(amount)
    );
}

fn dfs_select(
    notes: &[NoteSecret],
    amount: u128,
    index: usize,
    sum: u128,
    chosen: &mut Vec<usize>,
    best: &mut Option<CoinSelection>,
    visited: &mut usize,
) {
    *visited += 1;

    if *visited > 2_000_000 {
        return;
    }

    if sum >= amount {
        let change = sum - amount;
        let selected: Vec<NoteSecret> = chosen.iter().map(|&i| notes[i].clone()).collect();

        let should_replace = match best.as_ref() {
            None => true,
            Some(current) => {
                change < current.change
                    || (change == current.change && selected.len() < current.notes.len())
                    || (change == current.change
                        && selected.len() == current.notes.len()
                        && sum < current.input_sum)
            }
        };

        if should_replace {
            *best = Some(CoinSelection {
                notes: selected,
                input_sum: sum,
                change,
            });
        }

        return;
    }

    if index >= notes.len() || chosen.len() >= MAX_WITHDRAW_INPUTS {
        return;
    }

    if let Some(current) = best.as_ref() {
        if current.change == 0 {
            return;
        }
    }

    let remaining_slots = MAX_WITHDRAW_INPUTS - chosen.len();
    let max_possible = max_possible_from(notes, index, remaining_slots);

    if sum.saturating_add(max_possible) < amount {
        return;
    }

    chosen.push(index);
    dfs_select(
        notes,
        amount,
        index + 1,
        sum.saturating_add(notes[index].value),
        chosen,
        best,
        visited,
    );
    chosen.pop();

    dfs_select(notes, amount, index + 1, sum, chosen, best, visited);
}

fn max_possible_from(notes: &[NoteSecret], start: usize, slots: usize) -> u128 {
    notes
        .iter()
        .skip(start)
        .take(slots)
        .map(|note| note.value)
        .sum()
}

fn minimum_inputs_with_largest_first(notes: &[NoteSecret], amount: u128) -> usize {
    let mut sum = 0u128;

    for (idx, note) in notes.iter().enumerate() {
        sum = sum.saturating_add(note.value);

        if sum >= amount {
            return idx + 1;
        }
    }

    usize::MAX
}

pub fn format_xlm(stroops: u128) -> String {
    let whole = stroops / 10_000_000;
    let frac = stroops % 10_000_000;

    if frac == 0 {
        return format!("{whole} XLM");
    }

    let mut frac_text = format!("{frac:07}");
    while frac_text.ends_with('0') {
        frac_text.pop();
    }

    format!("{whole}.{frac_text} XLM")
}
