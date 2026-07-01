use dtoken_event_ledger::EventLedger;

#[test]
fn mock_ledger_has_event() {
    let ledger = EventLedger::mock_m1();
    assert_eq!(ledger.recent(20).len(), 1);
}
