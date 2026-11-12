use super::*;
use crate::testing::FakeHost;
use std::{cell::RefCell, rc::Rc};

/// A mod's own resource, which the game knows nothing of.
#[derive(Clone, Debug, PartialEq)]
struct Chemical(&'static str);

/// Tanks of up to 100 of a chemical each, shared with the test to read back.
#[derive(Clone)]
struct Tanks(Rc<RefCell<Vec<Option<(Chemical, u32)>>>>);

impl Tanks {
    fn new(held: Vec<Option<(Chemical, u32)>>) -> Self {
        Self(Rc::new(RefCell::new(held)))
    }
    fn held(&self) -> Vec<Option<(Chemical, u32)>> {
        self.0.borrow().clone()
    }
}

impl ResourceHandler<Chemical> for Tanks {
    fn size(&self, _: &mut dyn ServerHost) -> usize {
        self.0.borrow().len()
    }
    fn resource(&self, _: &mut dyn ServerHost, index: usize) -> Option<Chemical> {
        self.0.borrow().get(index)?.as_ref().map(|(c, _)| c.clone())
    }
    fn amount(&self, _: &mut dyn ServerHost, index: usize) -> u32 {
        self.0
            .borrow()
            .get(index)
            .cloned()
            .flatten()
            .map_or(0, |(_, a)| a)
    }
    fn capacity(&self, _: &mut dyn ServerHost, _: usize, _: &Chemical) -> u32 {
        100
    }
    fn insert(
        &self,
        _: &mut dyn ServerHost,
        _: &Transaction,
        index: usize,
        chemical: &Chemical,
        amount: u32,
    ) -> u32 {
        let mut tanks = self.0.borrow_mut();
        let Some(tank) = tanks.get_mut(index) else {
            return 0;
        };
        let held = match tank {
            Some((other, _)) if other != chemical => return 0,
            Some((_, held)) => *held,
            None => 0,
        };
        let went = amount.min(100 - held);
        if went > 0 {
            *tank = Some((chemical.clone(), held + went));
        }
        went
    }
    fn extract(
        &self,
        _: &mut dyn ServerHost,
        _: &Transaction,
        index: usize,
        chemical: &Chemical,
        amount: u32,
    ) -> u32 {
        let mut tanks = self.0.borrow_mut();
        let Some(tank) = tanks.get_mut(index) else {
            return 0;
        };
        let Some((held_chemical, held)) = tank.clone().filter(|(c, _)| c == chemical) else {
            return 0;
        };
        let came = amount.min(held);
        *tank = (held > came).then(|| (held_chemical, held - came));
        came
    }
}

const HYDROGEN: Chemical = Chemical("test:hydrogen");

#[test]
fn a_combined_handler_numbers_both_and_fills_the_second_when_the_first_is_full() {
    let full = Some((HYDROGEN, 100));
    let first = Tanks::new(vec![full.clone(), full.clone()]);
    let second = Tanks::new(vec![None, None]);
    let combined = CombinedResourceHandler::new(vec![
        Box::new(first.clone()) as Box<dyn ResourceHandler<Chemical>>,
        Box::new(second.clone()),
    ]);
    let mut fake = FakeHost::new();
    let host: &mut dyn ServerHost = &mut fake;
    assert_eq!(combined.size(host), 4);
    assert_eq!(combined.amount(host, 1), 100);
    assert_eq!(combined.resource(host, 2), None);
    let went =
        host.transaction(|host, tx| Ok::<_, ()>(combined.insert_any(host, tx, &HYDROGEN, 150)));
    assert_eq!(went, Ok(150));
    assert_eq!(first.held(), vec![full.clone(), full.clone()]);
    assert_eq!(second.held(), vec![full, Some((HYDROGEN, 50))]);
}

#[test]
fn insert_stacking_tops_up_a_holding_index_before_an_empty_one() {
    let tanks = Tanks::new(vec![None, Some((HYDROGEN, 10))]);
    let handler: &dyn ResourceHandler<Chemical> = &tanks;
    let mut fake = FakeHost::new();
    let host: &mut dyn ServerHost = &mut fake;
    let went =
        host.transaction(|host, tx| Ok::<_, ()>(handler.insert_stacking(host, tx, &HYDROGEN, 95)));
    assert_eq!(went, Ok(95));
    // The default `insert_any` would have filled index 0 first.
    assert_eq!(
        tanks.held(),
        vec![Some((HYDROGEN, 5)), Some((HYDROGEN, 100))]
    );
}

#[test]
fn the_comparator_reads_0_when_empty_15_when_full_and_vanillas_steps_between() {
    let mut fake = FakeHost::new();
    let host: &mut dyn ServerHost = &mut fake;
    let signal = |host: &mut dyn ServerHost, held| {
        (&Tanks::new(held) as &dyn ResourceHandler<Chemical>).comparator_signal(host)
    };
    assert_eq!(signal(&mut *host, vec![None, None]), 0);
    assert_eq!(
        signal(
            &mut *host,
            vec![Some((HYDROGEN, 100)), Some((HYDROGEN, 100))]
        ),
        15
    );
    // One unit in one of two tanks: any content gives at least 1.
    assert_eq!(signal(&mut *host, vec![Some((HYDROGEN, 1)), None]), 1);
    // Half full: floor(0.5 * 14) + 1.
    assert_eq!(signal(&mut *host, vec![Some((HYDROGEN, 100)), None]), 8);
}
