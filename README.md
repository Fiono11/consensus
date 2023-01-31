## Overview and Intuition

Right now in nano final votes are really final, meaning they do not change. Because of that, there are scenarios where 
consensus stalls and does not terminate, breaking the Termination property of consensus. To try to solve this issue we 
change a final vote of value v iif there is a more recent set of at least 2f + 1 initial votes of value v'. 
For example, if a node makes a final vote of value v in round r, it means that it received at least 2f + 1 initial votes 
of value v in a previous round r', r' < r. However, if it later receives at least 2f + 1 initial votes of value v' in a 
round r'', r'' > r', it changes the value of its final vote to v'.
We prove that this change achieves Termination and does not break the Agreement property of consensus in the formal
proofs. To run the consensus algorithm implementation with logs run ```cargo test -- --nocapture```.

### Values of Vote
- Zero
- One

### Types of Vote 
- Initial 
- Final 
- Decided 

### States of Vote
- Valid
- Invalid
- Pending

### Notes
- Only final and decided votes are validated
- All initial votes are considered valid, including the malicious ones, because there is no reliable way to detect them
- Final and decided votes have a proof round, the round in which they received the votes that justify them.
- The proof round of a decided vote of value v needs to have at least 2f + 1 final votes of value v for it to be considered valid.
- The proof round of a final vote of value v needs to have at least 2f + 1 initial votes for it to be considered valid.
- A valid vote can be considered invalid, however it will eventually be considered valid because all votes are eventually delivered.

### Decision Rules
Decide vote in round r after receiving at least 2f + 1 valid votes in round r-1 and the timer of round r expired:
- if there are at least 2f + 1 valid final votes of value v
  - decide value v
- if there is at least one valid decided vote of value v
  - decide v
- if there are at least 2f + 1 votes initial of value v
  - make a final vote of value v
- if there is at least one valid new final vote
  - if we already have an old final vote of value v'
    - if the new final vote has value v, is valid and has a more recent proof round
      - change the old final vote to the new final vote
    - else
      - make again a final vote of value v'
  - else
    - make a final vote equal to the valid final vote with the more recent proof round
- if initial zeros >= initial ones 
  - make an initial vote of value zero
- else
  - make an initial vote of value one

## Model

We use the same model as Tendermint [1]. We consider a system of processes that communicate by exchanging messages. 
Processes can be correct or faulty, where a faulty process can behave in an arbitrary way, i.e., we consider Byzantine 
faults. We assume that each process has some amount of voting power (voting power of a process can be 0). Processes in 
our model are not part of a single administrative domain; therefore we cannot enforce a direct network connectivity 
between all processes. Instead, we assume that each process is connected to a subset of processes called peers, such 
that there is an indirect communication channel between all correct processes. Communication between processes is 
established using a gossip protocol.

Formally, we model the network communication using a variant of the partially synchronous system model: in all
executions of the system there is a bound ∆ and an instant GST (Global Stabilization Time) such that all communication
among correct processes after GST is reliable and ∆-timely, i.e., if a correct process p sends message m at time t ≥ GST
to a correct process q, then q will receive m before t + ∆. In addition to the standard partially synchronous system
model, we assume an auxiliary property that captures gossip-based nature of communication.

**Gossip communication.** If a correct process p sends some message m at time t, all correct processes will receive m before
max{t, GST } + ∆. Furthermore, if a correct process p receives some message m at time t, all correct processes will
receive m before max{t, GST } + ∆.

The bound ∆ and GST are system parameters whose values are not required to be known for the safety of our algorithm.
Termination of the algorithm is guaranteed within a bounded duration after GST. In practice, the algorithm will work
correctly in the slightly weaker variant of the model where the system alternates between (long enough) good periods
(corresponds to the after GST period where system is reliable and ∆-timely) and bad periods (corresponds to the period
before GST during which the system is asynchronous and messages can be lost), but consideration of the GST model
simplifies the discussion.

**Goal.** Design a binary consensus algorithm that guarantees that every correct decides a value (Termination) and that
no two correct processes decide on different values (Agreement).

## **Safety proofs**

**Lemma 1.** For all f ≥ 0, any two sets of processes with voting power at least equal to 2f + 1 have at least one correct
process in common.

*Proof.* As the total voting power is equal to n = 3f +1, we have 2(2f +1) = n+f +1. This means that the intersection of
  two sets with the voting power equal to 2f + 1 contains at least f + 1 voting power in common, i.e., at least one
  correct process (as the total voting power of faulty processes is f). The result follows directly from this.

**Lemma 2.** No two correct processes decide on different values (Agreement).

*Proof.* Let round r0 be the first round such that some correct process p decides v. We now prove that if some correct
process q decides v′ in some round r ≥ r0, then v = v′.

In case r = r0, q has received at least 2f + 1 final votes of value v' in round r0, while p has received at least
2f + 1 final votes of value v in round r0. By Lemma 1 two sets of messages of voting power 2f + 1 intersect in at
least one correct process. As a correct process votes only once per round, then v = v′.

We prove the case r > r0 by contradiction. By the decision rules, p has received at least 2f + 1 voting power equivalent 
of final votes of value v in round r0, i.e., at least f+1 voting power equivalent correct processes have made a final 
vote of value v in round r0 and have sent those votes (i). Let denote this set of votes with C.

On the other side, q has received at least 2f + 1 voting power equivalent of final votes of value v' in round r. As the
voting power of all faulty processes is at most f, some correct process c has sent one of those votes. Therefore, c has 
received 2f + 1 initial votes of value v' in round r > r0.

By Lemma 1, a process from the set C has sent an initial vote of value v' in round r, a contradiction with (i), since a
correct node never changes a final vote to an initial vote.

**Lemma 3.** A correct node eventually sends a final vote.

*Proof.* After GST, by the Gossip communication property, every correct node receives all votes in a given round before
the timer expires and decides equally, since the conflicting votes are detected and discarded.

**Lemma 4.** All correct nodes eventually decide (Termination).

*Proof.* Since eventually a correct node sends a final vote (Lemma 3), after GST every correct node will eventually receive
  all the sets of at least 2f + 1 of initial votes and make the final vote with the value v of the set of the most
  recent round (you cannot have two different sets in the same round due to Lemma 1).
  Eventually all correct nodes will receive at least 2f + 1 final votes of value v and decide v.

## References

[1] Buchman, E.; Kwon, J.; Milosevic, Z. The latest gossip on BFT consensus. arXiv 2018, arXiv:1807.04938.
