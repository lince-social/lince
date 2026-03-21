== Transfer Proposals

#table(
  columns: 2,
  [transfer], [DATA TYPE],
  [id], [INTEGER],
  [records_received], [JSON],
  [records_contributed], [JSON],
  [agreement], [JSON],
  [agreement_time], [TIMESTAMP],
  [transfer_confirmation], [JSON],
  [transfer_time], [TIMESTAMP],
)

Transfer Proposals are a way of exchanging record quantities between different DNAs. It is thought to be used for executing economic exchanges
like buying goods and services, donations, organizing events, work tasks...

'records_received' is a collection of records and their quantities that will interact with our records, things you will receive.
'records_contributed' are the records you will contribute and their quantities, to the records of other parties, can be more
than one party. So you receive 1 in your 'Apple' Record's quantity, and give out 5 moneys to the other party involved.

'agreement' is a collection of agreement by all parties involved. 'agreement_time' is the moment every party agreed for the conditions of the trade,
who will receive what. 'transfer_confirmation'
is also a collection but with a confirmation from all parties that the transfer was successful, and 'transfer_time' for saving the event's moment.
