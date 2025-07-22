::  /ker/wallet/wallet: nockchain wallet
/=  bip39  /common/bip39
/=  slip10  /common/slip10
/=  m  /common/markdown/types
/=  md  /common/markdown/markdown
/=  transact  /common/tx-engine
/=  z   /common/zeke
/=  zo  /common/zoon
/=  dumb  /apps/dumbnet/lib/types
/=  *   /common/zose
/=  *  /common/wrapper
::
=>
=|  bug=_&
|%
::    $key: public or private key
::
::   both private and public keys are in serialized cheetah point form
::   they MUST be converted to base58 for export.
::
+$  key
  $~  [%pub p=*@ux]
  $%  [%pub p=@ux]
      [%prv p=@ux]
  ==
::    $coil: key and chaincode
::
::  a wallet consists of a collection of +coil (address and entropy pair). the
::  $cc (called a chain code elsewhere) allows for the deterministic
::  generation of child keys from a parent key without compromising other
::  branches of the hierarchy.
::
::  .key:  public or private key
::  .cc: associated entropy (chain code)
::
+$  coil  [%coil =key =cc]
::
::    $meta: stored metadata for a key
+$  meta
  $%  coil
      [%label @t]
      [%seed @t]
  ==
::
::    $keys: path indexed map for keys
::
::  path format for keys state:
::
::  /keys                                                        ::  root path (holds nothing in its fil)
::  /keys/[t/master]/[key-type]/m/[coil/key]                     ::  master key path
::  /keys/[t/master]/[key-type]/[ud/index]/[coil/key]            ::  derived key path
::  /keys/[t/master]/[key-type]/[ud/index]/[coil/key]            ::  specific key path
::  /keys/[t/master]/[key-type]/[ud/index]/label/[label/label]   ::  key label path for derived key
::  /keys/[t/master]/[key-type]/m/label/[label/label]            ::  key label path for master key
::  /keys/[t/master]/seed/[seed/seed-phrase]                     ::  seed-phrase path
::
::  Note the terminal entry of the path holds that value, this value is the
::  non-unit `fil` in the $axal definition
::
::  where:
::  - [t/master] is the base58 encoded master public key as @t
::  - m denotes the master key
::  - [ud/index] is the derivation index as @ud
::  - [key-type] is either %pub or %prv
::  - [coil/key] is the key and chaincode pair. key is in serialized
::               format as a @ux, NOT base58.
::  - [seed/seed-phrase] is the seed phrase as a tape
::  - [label/label] is a label value
::
::  master key is stored under 'm'.
::  derived keys use incrementing indices starting from 0 under their master-key and key-type
::  labels are stored as children of their associated keys.
::  seed is a seed phrase and is only stored as a child of [t/master]
::
+$  keys  $+(keys-axal (axal meta))
::
::    $draft-tree: structured tree of draft, input, and seed data
::
::  we use the axal structure to track the relationship between drafts,
::  inputs, and seeds. this allows us to navigate the tree and maintain
::  all the relationships without duplicating data.
::
::  paths in the draft-tree follow these conventions:
::
::  /draft/[draft-name]                    :: draft node
::  /draft/[draft-name]/input/[input-name] :: input in a draft
::  /input/[input-name]                    :: input node
::  /input/[input-name]/seed/[seed-name]   :: seed in an input
::  /seed/[seed-name]                      :: seed node
::
+$  draft-tree
  $+  wallet-draft-tree
  (axal draft-entity)
::    $draft-entity: entities stored in the draft tree
::
+$  draft-entity
  $%  [%draft =draft-name =draft]
      [%input =input-name =preinput]
      [%seed =seed-name =preseed]
  ==
::
::  +master: master key pair
++  master
  =<  form
  |%
    +$  form  (unit coil)
    ++  public
      |=  =form
      ?:  ?=(^ form)
        u.form
      ~|("master public key not found" !!)
    ::
    ++  to-b58
      |=  =form
      ^-  @t
      (crip (en:base58:wrap p.key:(public form)))
  --
::  $cc: chaincode
::
+$  cc  @ux
::  $balance: wallet balance
+$  balance
  $+  wallet-balance
  (z-map:zo nname:transact nnote:transact)
::
+$  ledger
  %-  list
  $:  name=nname:transact
      recipient=lock:transact
      gifts=coins:transact
      =timelock-intent:transact
  ==
::  $state: wallet state
::
+$  state
  $:  %0
      =balance
      hash-to-name=(z-map:zo hash:transact nname:transact)  ::  hash of note -> name of note (TODO: deprecated)
      name-to-hash=(z-map:zo nname:transact hash:transact)  ::  name of note -> hash of note (TODO: deprecated)
      receive-address=lock:transact
      =master
      =keys
      transactions=$+(transactions (map * transaction))
      last-block=(unit block-id:transact)
      peek-requests=$+(peek-requests (map @ud ?(%balance %block)))
      active-draft=(unit draft-name)
      active-input=(unit input-name)
      active-seed=(unit seed-name)             ::  currently selected seed
      draft-tree=draft-tree                    ::  structured tree of drafts, inputs, and seeds
      pending-commands=(z-map:zo @ud [phase=?(%block %balance %ready) wrapped=cause])  ::  commands waiting for sync
  ==
+$  seed-name   $~('default-seed' @t)
::
+$  draft-name  $~('default-draft' @t)
::
+$  input-name  $~('default-input' @t)
::
::  $transaction: TODO
::
+$  transaction
  $:  recipient=@ux
      amount=@ud
      status=?(%unsigned %signed %sent)
  ==
::
+$  cause
  $%  [%keygen entropy=byts salt=byts]
      [%derive-child i=@ hardened=? label=(unit @tas)]
      [%import-keys keys=(list (pair trek meta))]
      [%import-extended extended-key=@t]               ::  extended key string
      [%export-keys ~]
      [%export-master-pubkey ~]
      [%import-master-pubkey =coil]                    ::  base58-encoded pubkey + chain code
      [%send-tx dat=draft]
      [%list-notes-by-pubkey pubkey=@t]                ::  base58-encoded pubkey
      [%list-notes-by-pubkey-csv pubkey=@t]            ::  base58-encoded pubkey, CSV format
      $:  %simple-spend
          names=(list [first=@t last=@t])              ::  base58-encoded name hashes
          recipients=(list [m=@ pks=(list @t)])        ::  base58-encoded locks
          gifts=(list coins:transact)                  ::  number of coins to spend
          fee=coins:transact                           ::  fee
          index=(unit @ud)                             ::  index of child key to spend from
          =timelock-intent:transact                    ::  timelock constraint
      ==
      [%sign-tx dat=draft index=(unit @ud) entropy=@]
      [%list-pubkeys ~]
      [%list-notes ~]
      [%show-seedphrase ~]
      [%show-master-pubkey ~]
      [%show-master-privkey ~]
      [%show =path]
      [%gen-master-privkey seedphrase=@t]
      [%gen-master-pubkey privkey-b58=@t cc-b58=@t]
      [%update-balance ~]
      [%update-block ~]
      [%sync-run wrapped=cause]                         ::  run command after sync completes
      $:  %scan
          master-pubkey=@t              ::  base58 encoded master public key to scan for
          search-depth=$~(100 @ud)      ::  how many addresses to scan (default 100)
          include-timelocks=$~(%.n ?)   ::  include timelocked notes (default false)
          include-multisig=$~(%.n ?)    ::  include notes with multisigs (default false)
      ==
      [%advanced-spend advanced-spend]
      [%file %write path=@t contents=@t success=?]
      npc-cause
  ==
::
+$  advanced-spend
  $%  [%seed advanced-spend-seed]
      [%input advanced-spend-input]
      [%draft advanced-spend-draft]
  ==
::
+$  advanced-spend-seed
  $%  [%new name=@t]                          ::  new empty seed in draft
      $:  %set-name
          seed-name=@t
          new-name=@t
      ==
      $:  %set-source                                  ::  set .output-source
          seed-name=@t
          source=(unit [hash=@t is-coinbase=?])
      ==
      $:  %set-recipient                               ::  set .recipient
          seed-name=@t
          recipient=[m=@ pks=(list @t)]
      ==
      $:  %set-timelock                                ::  set .timelock-intent
          seed-name=@t
          absolute=timelock-range:transact
          relative=timelock-range:transact
      ==
      $:  %set-gift
          seed-name=@t
          gift=coins:transact
      ==
      $:  %set-parent-hash
          seed-name=@t
          parent-hash=@t
      ==
      $:  %set-parent-hash-from-name
          seed-name=@t
          name=[@t @t]
      ==
      $:  %print-status                                ::  do the needful
          seed-name=@t
      ==
  ==
::  $seed-mask: tracks which fields of a $seed:transact have been set
::
::    this might have been better as a "unitized seed" but would have been
::    much more annoying to read the code
+$  seed-mask
  $~  [%.n %.n %.n %.n %.n]
  $:  output-source=?
      recipient=?
      timelock-intent=?
      gift=?
      parent-hash=?
  ==
::  $preseed: a $seed:transact in process of being built
+$  preseed  [name=@t (pair seed:transact seed-mask)]
::
::  $spend-mask: tracks which field of a $spend:transact have been set
+$  spend-mask
  $~  [%.n %.n %.n]
  $:  signature=?
      seeds=?
      fee=?
  ==
::
+$  advanced-spend-input
  ::  there is only one right way to create an $input from a $spend, so we don't need
  ::  the mask or other commands.
  $%  [%new name=@t]                                   :: new empty input
      $:  %set-name
          input-name=@t
          new-name=@t
      ==
      $:  %add-seed
          input-name=@t
          seed-name=@t
      ==
      $:  %set-fee
          input-name=@t
          fee=coins:transact
      ==
      $:  %set-note-from-name                          ::  set .note using .name
          input-name=@t
          name=[@t @t]
      ==
      $:  %set-note-from-hash                          ::  set .note using hash
          input-name=@t
          hash=@t
      ==
      $:  %derive-note-from-seeds                      ::  derive note from seeds
          input-name=@t
      ==
      $:  %remove-seed
          input-name=@t
          seed-name=@t
      ==
      $:  %remove-seed-by-hash
          input-name=@t
          hash=@t
      ==
      $:  %print-status
          input-name=@t
      ==
  ==
::
+$  input-mask
  $~  [%.n *spend-mask]
  $:  note=?
      spend=spend-mask
  ==
::
+$  preinput  [name=@t (pair input:transact input-mask)]
::
+$  draft  [name=@t p=inputs:transact]
::
+$  advanced-spend-draft
  $%  [%new name=@t]                                    ::  new input draft
      $:  %set-name
          draft-name=@t
          new-name=@t
      ==
      $:  %add-input
          draft-name=@t
          input-name=@t
      ==
      $:  %remove-input
          draft-name=@t
          input-name=@t
      ==
      $:  %remove-input-by-name
          draft-name=@t
          name=[first=@t last=@t]
      ==
      [%print-status =draft-name]                            ::  print draft status
  ==
::
+$  npc-cause
  $%  [%npc-bind pid=@ result=*]
  ==
::
+$  effect
  $~  [%npc 0 %poke %fact *fact:dumb]
  $%  file-effect
      [%markdown @t]
      [%raw *]
      [%npc pid=@ npc-effect]
      [%exit code=@]
  ==
::
+$  file-effect
  $%
    [%file %read path=@t]
    [%file %write path=@t contents=@]
  ==
::
+$  npc-effect
  $%  [%poke $>(%fact cause:dumb)]
      [%peek path]
  ==
::
::TODO this probably shouldnt live in here
::
++  print
  |=  nodes=markdown:m
  ^-  (list effect)
  ~[(make-markdown-effect nodes)]
::
++  warn
  |*  meg=tape
  |*  *
  ?.  bug  +<
  ~>  %slog.[1 (cat 3 'wallet: warning: ' (crip meg))]
  +<
::
++  debug
  |*  meg=tape
  |*  *
  ?.  bug  +<
  ~>  %slog.[2 (cat 3 'wallet: debug: ' (crip meg))]
  +<
::
++  moat  (keep state)
::
::  +timelock-helpers: helper functions for creating timelock-intents
::
++  timelock-helpers
  |%
  ::  +make-relative-timelock-intent: create relative timelock-intent
  ::
  ::    min-rel: minimum pages after note creation before spendable
  ::    max-rel: maximum pages after note creation when spendable
  ++  make-relative-timelock-intent
    |=  [min-rel=(unit @ud) max-rel=(unit @ud)]
    ^-  timelock-intent:transact
    `[*timelock-range:transact (new:timelock-range:transact min-rel max-rel)]
  ::
  ::  +make-absolute-timelock-intent: create absolute timelock-intent
  ::
  ::    min-abs: minimum absolute page number when spendable
  ::    max-abs: maximum absolute page number when spendable
  ++  make-absolute-timelock-intent
    |=  [min-abs=(unit @ud) max-abs=(unit @ud)]
    ^-  timelock-intent:transact
    `[(new:timelock-range:transact min-abs max-abs) *timelock-range:transact]
  ::
  ::  +make-combined-timelock-intent: create timelock-intent with both absolute and relative
  ++  make-combined-timelock-intent
    |=  $:  min-abs=(unit @ud)
            max-abs=(unit @ud)
            min-rel=(unit @ud)
            max-rel=(unit @ud)
        ==
    ^-  timelock-intent:transact
    `[(new:timelock-range:transact min-abs max-abs) (new:timelock-range:transact min-rel max-rel)]
  ::
  ::  +no-timelock: convenience function for no timelock constraint
  ++  no-timelock
    ^-  timelock-intent:transact
    *timelock-intent:transact
  --
::
::  +edit: modify inputs
++  edit
  |_  =state
  ::
  +*  inp
    ^-  preinput
    ?~  active-input.state
      %-  (debug "no active input set!")
      *preinput
    =/  input-result  (~(get-input plan draft-tree.state) u.active-input.state)
    ?~  input-result
      ~|("active input not found in draft-tree" !!)
    u.input-result
  ::    +add-seed: add a seed to the input
  ::
  ++  add-seed
    |=  =seed:transact
    ^-  [(list effect) ^state]
    ?:  (~(has z-in:zo seeds.spend.p.inp) seed)
      :_  state
      %-  print
      %-  need
      %-  de:md
      %-  crip
      """
      ##  add-seed

      **seed already exists in .spend**
      """
    =/  pre=preinput  inp
    =/  =preinput
      %=    pre
          seeds.spend.p
        %.  seed
        ~(put z-in:zo seeds.spend.p.pre)
        ::
        seeds.spend.q  %.y
      ==
    =.  active-input.state  (some name.pre)
    ::
    =/  input-name=input-name  (need active-input.state)
    =.  draft-tree.state
      (~(add-input plan draft-tree.state) input-name preinput)
    ::  if active-seed is set, link it to this input
    =.  draft-tree.state
      ?:  ?=(^ active-seed.state)
        (~(link-seed-to-input plan draft-tree.state) input-name u.active-seed.state)
      draft-tree.state
    `state
  ::
  ++  remove-seed
    |=  =seed:transact
    ^-  [(list effect) ^state]
    ?.  (~(has z-in:zo seeds.spend.p.inp) seed)
      :_  state
      %-  print
      %-  need
      %-  de:md
      %-  crip
      """
      ##  remove-seed

      **seed not found in .spend**
      """
    =/  pre=preinput  inp
    =.  seeds.spend.p.pre
      %.  seed
      ~(del z-in:zo seeds.spend.p.pre)
    =.  draft-tree.state
      =/  input-name=input-name  (need active-input.state)
      (~(add-input plan draft-tree.state) input-name pre)
    `state
  --
::
::  +draw: modify drafts
++  draw
  |_  =state
  +*  df
    ^-  draft
    ?>  ?=(^ active-draft.state)
    =/  draft-result  (~(get-draft plan draft-tree.state) u.active-draft.state)
    ?~  draft-result
      *draft
    u.draft-result
  ::    +add-input: add an input to the draft
  ::
  ++  add-input
    |=  =input:transact
    ^-  [(list effect) ^state]
    =/  =draft  df
    =/  =input-name
      =+  (to-b58:nname:transact name.note.input)
      %-  crip
      "{<first>}-{<last>}"
    ?:  (~(has z-by:zo p.df) name.note.input)
      :_  state
      %-  print
      %-  need
      %-  de:md
      %-  crip
      """
      ##  add-input

      **input already exists in .draft**

      draft already has input with note name: {<input-name>}
      """
    =/  active-draft=draft-name  (need active-draft.state)
    =.  p.draft
      %-  ~(put z-by:zo p.draft)
      :-  name.note.input
      input
    =.  draft-tree.state
      %.  [active-draft draft]
      ~(add-draft plan draft-tree.state)
    =.  draft-tree.state
      %.  [active-draft input-name]
      ~(link-input-to-draft plan draft-tree.state)
    write-draft
  ::
  ++  write-draft
    ^-  [(list effect) ^state]
    =?  active-draft.state  ?=(~ active-draft.state)  (some *draft-name)
    ?>  ?=(^ active-draft.state)
    =/  =draft  df
    =.  draft-tree.state  (~(add-draft plan draft-tree.state) u.active-draft.state draft)
    =/  dat-jam  (jam draft)
    =/  path=@t  (crip "drafts/{(trip u.active-draft.state)}.draft")
    =/  effect  [%file %write path dat-jam]
    :_  state
    ~[effect [%exit 0]]
  --
::
::  Convenience wrapper door for slip10 library
::  ** Never use slip10 directly in the wallet **
++  s10
  |_  bas=base:slip10
  ++  gen-master-key
    |=  [entropy=byts salt=byts]
    =/  argon-byts=byts
      :-  32
      %+  argon2-nockchain:argon2:crypto
        entropy
      salt
    =/  memo=tape  (from-entropy:bip39 argon-byts)
    %-  (debug "memo: {memo}")
    :-  (crip memo)
    (from-seed:slip10 [64 (to-seed:bip39 memo "")])
::
  ++  from-seed
    |=  =byts
    (from-seed:slip10 byts)
::
  ++  from-private
    |=  =keyc:slip10
    (from-private:slip10 keyc)
::
  ++  from-public
    |=  =keyc:slip10
    (from-public:slip10 keyc)
  ::
  ::  derives the i-th child key(s) from a parent key.
  ::  index i can be any child index. returns the door
  ::  with the door sample modified with the values
  ::  corresponding to the key. the core sample can then
  ::  be harvested for keys.
  ::
  ++  derive
    |=  [parent=coil i=@u]
    ?-    -.key.parent
        %pub
      =>  [cor=(from-public [p.key cc]:parent) i=i]
      (derive:cor i)
    ::
        %prv
      =>  [cor=(from-private [p.key cc]:parent) i=i]
      (derive:cor i)
    ==
  ::
  ++  from-extended-key
    |=  key=@t
    (from-extended-key:slip10 key)
  --
::
++  vault
  |_  =state
  ++  base-path  ^-  trek
    ?~  master.state
      ~|("base path not accessible because master not set" !!)
    /keys/[t/(to-b58:master master.state)]
  ::
  ++  seed-path  ^-  trek
    (welp base-path /seed)
  ::
  ++  set-receive-address
    ^-  ^state
    ?~  master.state
      state
    =/  master-coil=coil  (public:master master.state)
    ?>  ?=(%pub -.key.master-coil)
    =/  pubkey=schnorr-pubkey:transact
      pub:(from-public:s10 [p.key cc]:master-coil)
    =/  =lock:transact  (new:lock:transact pubkey)
    state(receive-address lock)
  ::
  ++  assert-receive-address
    ^-  lock:transact
    ?:  =(receive-address.state *lock:transact)
      ~|("receive-address not set - this is a bug" !!)
    receive-address.state
  ::
  ++  has
    |_  key-type=?(%pub %prv)
    ++  key-path  ^-  trek
      (welp base-path ~[key-type])
    ::
    ++  master
      ^-  ?
      =/  =trek  (welp key-path /m)
      (~(has of keys.state) trek)
    --
  ++  get
    |_  key-type=?(%pub %prv)
    ::
    ++  key-path  ^-  trek
      (welp base-path ~[key-type])
    ::
    ++  master
      ^-  coil
      =/  =trek  (welp key-path /m)
      =/  =meta  (~(got of keys.state) trek)
      :: check if private key matches public key
      ?>  ?=(%coil -.meta)
      ?:  ?=(%prv key-type)
        =/  public-key=@
          public-key:(from-private:s10 [p.key cc]:meta)
        ~&  >  ['public-key' public-key]
        ~&  >  ['master-pub' p.key:(public:^master master.state)]
        ?:  =(public-key p.key:(public:^master master.state))
          meta
        ~|("private key does not match public key" !!)
      meta
    ::
    ++  by-index
      |=  index=@ud
      ^-  coil
      =/  =trek  (welp key-path /[ud/index])
      =/  =meta  (~(got of keys.state) trek)
      ?>  ?=(%coil -.meta)
      meta
    ::
    ++  seed
      ^-  meta
      (~(got of keys.state) seed-path)
    ::
    ++  by-label
      |=  label=@t
      %+  murn  keys
      |=  [t=trek =meta]
      ?:(&(?=(%label -.meta) =(label +.meta)) `t ~)
    ::
    ++  keys
      ^-  (list [trek meta])
      =/  subtree
        %-  ~(kids of keys.state)
        key-path
      ~(tap by kid.subtree)
    ::
    ++  coils
      ^-  (list coil)
      %+  murn  keys
      |=  [t=trek =meta]
      ^-  (unit coil)
      ;;  (unit coil)
      ?:(=(%coil -.meta) `meta ~)
    --
  ::
  ++  put
    |%
    ::
    ++  seed
      |=  seed-phrase=@t
      ^-  (axal meta)
      %-  ~(put of keys.state)
      [seed-path [%seed seed-phrase]]
    ::
    ++  key
      |=  [=coil index=(unit @) label=(unit @t)]
      ^-  (axal meta)
      =/  key-type=@tas  -.key.coil
      =/  suffix=trek
        ?@  index
          /[key-type]/m
        /[key-type]/[ud/u.index]
      =/  key-path=trek  (welp base-path suffix)
      %-  (debug "adding key at {(en-tape:trek key-path)}")
      =.  keys.state  (~(put of keys.state) key-path coil)
      ?~  label
        keys.state
      %+  ~(put of keys.state)
        (welp key-path /label)
      label/u.label
    --
  ::
  ++  get-note
    |=  name=nname:transact
    ^-  nnote:transact
    ?:  (~(has z-by:zo balance.state) name)
      (~(got z-by:zo balance.state) name)
    ~|("note not found: {<name>}" !!)
  ::
  ::  TODO: way too slow, need a better way to do this or
  ::  remove entirely in favor of requiring note names in
  ::  the causes where necessary.
  ++  find-name-by-hash
    |=  has=hash:transact
    ^-  (unit nname:transact)
    =/  notes=(list [name=nname:transact note=nnote:transact])
      ~(tap z-by:zo balance.state)
    |-
    ?~  notes  ~
    ?:  =((hash:nnote:transact note.i.notes) has)
      `name.i.notes
    $(notes t.notes)
  ::
  ++  get-note-from-hash
    |=  has=hash:transact
    ^-  nnote:transact
    =/  name=(unit nname:transact)  (find-name-by-hash has)
    ?~  name
      ~|("note with hash {<(to-b58:hash:transact has)>} not found in balance" !!)
    (get-note u.name)
  ::
  ++  generate-pid
    |=  peek-type=?(%balance %block)
    ^-  (unit @ud)
    =/  used-pids=(list @ud)
      ~(tap in ~(key by peek-requests.state))
    =/  max-pid=@ud
      (roll used-pids max)
    =/  next-pid=@ud  +(max-pid)
    ?:  =(next-pid 0)  `1  :: handle wraparound
    `next-pid
  ::
  ::  +derive-child: derives the i-th hardened/unhardened child key(s)
  ::
  ::    derives the i-th child from the master key. for hardened keys,
  ::    (bex 31) should be already added to `i`.
  ::
  ++  derive-child
    |=  i=@u
    ^-  (set coil)
    ?:  (gte i (bex 32))
      ~|("Child index {<i>} out of range. Child indices are capped to values between [0, 2^32)" !!)
    ?~  master.state
      ~|("No master keys available for derivation" !!)
    =;  coils=(list coil)
      (silt coils)
    =/  hardened  (gte i (bex 31))
    ::
    ::  Grab the prv master key if it exists (cold wallet)
    ::  otherwise grab the pub master key (hot wallet).
    =/  parent=coil
      ?:  ~(master has %prv)
        ~(master get %prv)
      ~(master get %pub)
    ?:  hardened
      ?>  ?=(%prv -.key.parent)
      ::
      =>  (derive:s10 parent i)
      :~  [%coil [%prv private-key] chain-code]
          [%coil [%pub public-key] chain-code]
      ==
    ::
    ::  if unhardened, we just assert that they are within the valid range
    ?:  (gte i (bex 31))
      ~|("Unhardened child index {<i>} out of range. Indices are capped to values between [0, 2^31)" !!)
    ?-    -.key.parent
     ::  if the parent is a private key, we can derive the unhardened prv and pub child
        %prv
      =>  (derive:s10 parent i)
      :~  [%coil [%prv private-key] chain-code]
          [%coil [%pub public-key] chain-code]
      ==
    ::
     ::  if the parent is a public key, we can only derive the unhardened pub child
        %pub
      =>  (derive:s10 parent i)
      ~[[%coil [%pub public-key] chain-code]]
    ==
  --
::    +plan: core for managing draft relationships
::
::  provides methods for adding, removing, and navigating the draft tree.
::  uses the axal structure to maintain relationships between drafts, inputs,
::  and seeds.
::
++  plan
  |_  tree=draft-tree
  ::
  ::  +get-draft: retrieve a draft by name
  ::
  ++  get-draft
    |=  name=draft-name
    ^-  (unit draft)
    =/  res  (~(get of tree) /draft/[name])
    ?~  res  ~
    ?.  ?=(%draft -.u.res)  ~
    `draft.u.res
  ::    +get-input: retrieve an input by name
  ::
  ++  get-input
    |=  name=input-name
    ^-  (unit preinput)
    =/  res  (~(get of tree) /input/[name])
    ?~  res  ~
    ?.  ?=(%input -.u.res)  ~
    `preinput.u.res
  ::    +get-seed: retrieve a seed by name
  ::
  ++  get-seed
    |=  name=seed-name
    ^-  (unit preseed)
    =/  res  (~(get of tree) /seed/[name])
    ?~  res  ~
    ?.  ?=(%seed -.u.res)  ~
    `preseed.u.res
  ::    +add-draft: add a new draft
  ::
  ++  add-draft
    |=  [name=draft-name =draft]
    ^-  draft-tree
    =/  entity  [%draft name draft]
    (~(put of tree) /draft/[name] entity)
  ::    +add-input: add a new input
  ::
  ++  add-input
    |=  [name=input-name =preinput]
    ^-  draft-tree
    =/  entity  [%input name preinput]
    (~(put of tree) /input/[name] entity)
  ::    +add-seed: add a new seed
  ::
  ++  add-seed
    |=  [name=seed-name =preseed]
    ^-  draft-tree
    =/  entity  [%seed name preseed]
    (~(put of tree) /seed/[name] entity)
  ::    +link-input-to-draft: link an input to a draft
  ::
  ++  link-input-to-draft
    |=  [draft-name=draft-name input-name=input-name]
    ^-  draft-tree
    =/  input-entity  (~(get of tree) /input/[input-name])
    ?~  input-entity  tree
    ?.  ?=(%input -.u.input-entity)  tree
    (~(put of tree) /draft/[draft-name]/input/[input-name] u.input-entity)
  ::    +link-seed-to-input: link a seed to an input
  ::
  ++  link-seed-to-input
    |=  [input-name=input-name seed-name=seed-name]
    ^-  draft-tree
    =/  seed-entity  (~(get of tree) /seed/[seed-name])
    ?~  seed-entity  tree
    ?.  ?=(%seed -.u.seed-entity)  tree
    (~(put of tree) /input/[input-name]/seed/[seed-name] u.seed-entity)
  ::    +unlink-input-from-draft: remove an input from a draft
  ::
  ++  unlink-input-from-draft
    |=  [draft-name=draft-name input-name=input-name]
    ^-  draft-tree
    (~(del of tree) /draft/[draft-name]/input/[input-name])
  ::    +unlink-seed-from-input: remove a seed from an input
  ::
  ++  unlink-seed-from-input
    |=  [input-name=input-name seed-name=seed-name]
    ^-  draft-tree
    (~(del of tree) /input/[input-name]/seed/[seed-name])
  ::    +list-draft-inputs: list all inputs in a draft
  ::
  ++  list-draft-inputs
    |=  name=draft-name
    ^-  (list input-name)
    =/  kids  (~(kid of tree) /draft/[name])
    %+  murn  ~(tap in ~(key by kids))
    |=  pax=pith
    ^-  (unit input-name)
    =/  pax=path  (pout pax)
    ?>  ?=([%input *] pax)
    ?>  ?=(^ t.pax)
    `i.t.pax
  ::    +list-input-seeds: list all seeds in an input
  ::
  ++  list-input-seeds
    |=  name=input-name
    ^-  (list seed-name)
    =/  kids  (~(kid of tree) /input/[name])
    %+  murn  ~(tap in ~(key by kids))
    |=  pax=pith
    ^-  (unit seed-name)
    =/  pax=path  (pout pax)
    ?:  &(?=([%seed *] pax) ?=(^ t.pax))
      `i.t.pax
    ~
  ::    +list-all-drafts: list all draft names
  ::
  ++  list-all-drafts
    ^-  (list draft-name)
    =/  kids  (~(kid of tree) /draft)
    %+  murn  ~(tap in ~(key by kids))
    |=  pax=pith
    ^-  (unit draft-name)
    =/  pax=path  (pout pax)
    ?:  ?=(^ pax)
      `i.pax
    ~
  ::    +list-all-inputs: list all input names
  ::
  ++  list-all-inputs
    ^-  (list input-name)
    =/  kids  (~(kid of tree) /input)
    %+  murn  ~(tap in ~(key by kids))
    |=  pax=pith
    ^-  (unit input-name)
    =/  pax=path  (pout pax)
    ?:  ?=(^ pax)
      `i.pax
    ~
  ::    +list-all-seeds: list all seed names
  ::
  ++  list-all-seeds
    ^-  (list seed-name)
    =/  kids  (~(kid of tree) /seed)
    %+  murn  ~(tap in ~(key by kids))
    |=  pax=pith
    ^-  (unit seed-name)
    =/  pax=path  (pout pax)
    ?:  &(?=([%seed *] pax) ?=(^ t.pax))
      `i.pax
    ~
  ::    +remove-draft: completely remove a draft and its associations
  ::
  ++  remove-draft
    |=  name=draft-name
    ^-  draft-tree
    (~(lop of tree) /draft/[name])
  ::    +remove-input: completely remove an input and its associations
  ::
  ++  remove-input
    |=  name=input-name
    ^-  draft-tree
    (~(lop of tree) /input/[name])
  ::    +remove-seed: completely remove a seed and its associations
  ::
  ++  remove-seed
    |=  name=seed-name
    ^-  draft-tree
    (~(lop of tree) /seed/[name])
  --
::
++  make-markdown-effect
  |=  nodes=markdown:m
  [%markdown (crip (en:md nodes))]
::
++  display-poke
  |=  =cause
  ^-  effect
  =/  nodes=markdown:m
  %-  need
  %-  de:md
  %-  crip
  """
  ## poke
  {<cause>}
  """
  (make-markdown-effect nodes)
::
++  display-note-cord
  |=  note=nnote:transact
  ^-  @t
  %^  cat  3
   ;:  (cury cat 3)
      '''

      ## details

      '''
      '- name: '
      =+  (to-b58:nname:transact name.note)
      :((cury cat 3) '[' first ' ' last ']')
      '\0a- assets: '
      (scot %ui assets.note)
      '\0a- block height: '
      (scot %ui origin-page.note)
      '\0a- source: '
      (to-b58:hash:transact p.source.note)
      '\0a## lock'
      '\0a- m: '
      (scot %ui m.lock.note)
      '\0a- signers: '
    ==
  %-  crip
  %+  join  ' '
  (serialize-lock lock.note)
::
++  serialize-lock
  |=  =lock:transact
  ^-  (list @t)
  ~+
  pks:(to-b58:lock:transact lock)
::
++  display-note
  |=  note=nnote:transact
  ^-  markdown:m
  %-  need
  %-  de:md
  (display-note-cord note)
::
++  show
  |=  [=state =path]
  ^-  [(list effect) ^state]
  |^
  ?+    path  !!
      [%balance ~]
    :-  ~[(display-balance balance.state)]
    state
  ::
      [%receive-address ~]
    :-  (display-receive-address receive-address.state)
    state
  ::
      [%state ~]
    :-  display-state
    state
  ==
  ++  display-balance
    |=  =balance
    ^-  effect
    =/  nodes=markdown:m
    %-  need
    %-  de:md
    %-  crip
    """
    ## balance
    {<balance>}
    """
    (make-markdown-effect nodes)
  ::
  ++  display-receive-address
    |=  address=lock:transact
    ^-  (list effect)
    =/  nodes=markdown:m
    %-  need
    %-  de:md
    %-  crip
    """
    ## receive address
    {<address>}
    """
    ~[(make-markdown-effect nodes)]
  ::
  ++  display-state
    ^-  (list effect)
    =/  nodes=markdown:m
    %-  need
    %-  de:md
    %-  crip
    """
    ## state
    - last block: {<last-block.state>}
    """
    ~[(make-markdown-effect nodes)]
  --
  ::
  ++  ui-to-tape
    |=  @
    ^-  tape
    %-  trip
    (rsh [3 2] (scot %ui +<))
--
::
%-  (moat &)
^-  fort:moat
|_  =state
+*  v  ~(. vault state)
    d  ~(. draw state)
    e  ~(. edit state)
    p  ~(. plan draft-tree.state)
::
++  load
  |=  arg=^state
  ^-  ^state
  =.  arg  set-receive-address:~(. vault arg)
  arg
::
++  peek
  |=  =path
  ^-  (unit (unit *))
  %-  (debug "peek: {<state>}")
  ?+    path  ~
    ::
      [%balance ~]
    ``balance.state
    ::
      [%receive-address ~]
    ``receive-address.state
    ::
      [%state ~]
    ``state
  ==
::
++  poke
  |=  =ovum:moat
  |^
  ^-  [(list effect) ^state]
  %-  (warn "debug printing may expose sensitive information")
  =/  =wire  wire.ovum
  =+  [@ src=@ta ver=@ rest=*]=wire.ovum
  %-  (debug "wire: {<[src ver `path`rest]>}")
  =/  cause=(unit cause)
    %-  (soft cause)
    cause.input.ovum
  =/  success  [%exit 0]
  =/  failure  [%exit 1]
  ?~  cause
    %-  (debug "input does not have a proper cause: {<cause.input.ovum>}")
    [~[failure] state]
  =/  cause  u.cause
  %-  (debug "cause: {<-.cause>}")
  =;  process
    =^  effs  state  process
    ::  check for pending balance commands and execute them
    =^  pending-effs  state  handle-pending-commands
    [(weld effs pending-effs) state]
  ?-  -.cause
      %npc-bind              (handle-npc cause)
      %show                  (show state path.cause)
      %keygen                (do-keygen cause)
      %derive-child          (do-derive-child cause)
      %sign-tx               (do-sign-tx cause)
      %scan                  (do-scan cause)
      %list-notes            (do-list-notes cause)
      %list-notes-by-pubkey  (do-list-notes-by-pubkey cause)
      %list-notes-by-pubkey-csv  (do-list-notes-by-pubkey-csv cause)
      %simple-spend          (do-simple-spend cause)
      %update-balance        (do-update-balance cause)
      %update-block          (do-update-block cause)
      %import-keys           (do-import-keys cause)
      %import-extended       (do-import-extended cause)
      %export-keys           (do-export-keys cause)
      %export-master-pubkey  (do-export-master-pubkey cause)
      %import-master-pubkey  (do-import-master-pubkey cause)
      %gen-master-privkey    (do-gen-master-privkey cause)
      %gen-master-pubkey     (do-gen-master-pubkey cause)
      %send-tx               (do-send-tx cause)
      %list-pubkeys          (do-list-pubkeys cause)
      %sync-run              (do-sync-run cause)
      %show-seedphrase       (do-show-seedphrase cause)
      %show-master-pubkey    (do-show-master-pubkey cause)
      %show-master-privkey   (do-show-master-privkey cause)
    ::
      %advanced-spend
    ?-  +<.cause
      %seed   (do-advanced-spend-seed +>.cause)
      %input  (do-advanced-spend-input +>.cause)
      %draft  (do-advanced-spend-draft +>.cause)
    ==
    ::
      %file
    ?>  ?=(%write +<.cause)
    ~&  >  "%file %write: {<cause>}"
    [[%exit 0]~ state]
  ==
  ::
  ++  handle-npc
    |=  =npc-cause
    ^-  [(list effect) ^state]
    %-  (debug "handle-npc: {<-.npc-cause>}")
    ?-    -.npc-cause
        %npc-bind
      =/  pid  pid.npc-cause
      =/  peek-type
        ?.  (~(has by peek-requests.state) pid)
          ~|("no peek request found for pid: {<pid>}" !!)
        (~(got by peek-requests.state) pid)
      =/  result  result.npc-cause
      ?-  peek-type
          %balance
        =/  softed=(unit (unit (unit (z-map:zo nname:transact nnote:transact))))
          %-  (soft (unit (unit (z-map:zo nname:transact nnote:transact))))
          result
        =.  peek-requests.state
          (~(del by peek-requests.state) pid)
        ?~  softed
          %-  (debug "handle-npc: %balance: could not soft result")
          [[%exit 0]~ state]
        =/  balance-result=(unit (unit _balance.state))  u.softed
        ?~  balance-result
          %-  (warn "%update-balance did not return a result: bad path")
          [[%exit 0]~ state]
        ?~  u.balance-result
          %-  (warn "%update-balance did not return a result: nothing")
          [[%exit 0]~ state]
        ?~  u.u.balance-result
          %-  (warn "%update-balance did not return a result: empty result")
          [[%exit 0]~ state]
        =.  balance.state  u.u.balance-result
        %-  (debug "balance state updated!")
        ::  move each command from balance phase to ready phase
        =/  balance-commands=(list [pid=@ud [phase=?(%block %balance %ready) wrapped=cause]])
          %+  skim  ~(tap z-by:zo pending-commands.state)
          |=  [pid=@ud [phase=?(%block %balance %ready) wrapped=cause]]
          =(phase %balance)
        =.  pending-commands.state
          %-  ~(gas z-by:zo pending-commands.state)
          %+  turn  balance-commands
          |=  [pid=@ud [phase=?(%block %balance %ready) wrapped=cause]]
          [pid [%ready wrapped]]
        ::
        ::  the top-level poke arm should check for pending commands
        ::  and execute them as appropriate
        %-  (debug "handle-npc: %balance: balance updated, pending commands ready for execution")
        [~ state]
        ::
          %block
        %-  (debug "handle-npc: %block")
        =/  softed=(unit (unit (unit (unit block-id:transact))))
          %-  (soft (unit (unit (unit block-id:transact))))
          result
        =.  peek-requests.state
          (~(del by peek-requests.state) pid)
        ?~  softed
          %-  (warn "handle-npc: %block: could not soft result")
          [[%exit 0]~ state]
        =/  block-result  u.softed
        %-  (debug "handle-npc: %block: {<block-result>}")
        %-  (debug "handle-npc: %block: {<peek-requests.state>}")
        ?~  block-result
          %-  (warn "handle-npc: %block: bad path")
          [[%exit 0]~ state]
        ?~  u.block-result
          %-  (warn "handle-npc: %block: nothing")
          [[%exit 0]~ state]
        ?~  u.u.block-result
          %-  (warn "handle-npc: %block: empty result")
          [[%exit 0]~ state]
        %-  (debug "handle-npc: %block: found block")
        %-  (debug "handle-npc: {<(to-b58:block-id:transact (need u.u.block-result))>}")
        %-  (debug "handle-npc: hash: {<(to-b58:hash:transact (need u.u.block-result))>}")
        =.  last-block.state  u.u.block-result
        ::  move each command from block phase to balance phase
        =/  block-commands=(list [pid=@ud [phase=?(%block %balance %ready) wrapped=cause]])
          %+  skim  ~(tap z-by:zo pending-commands.state)
          |=  [pid=@ud [phase=?(%block %balance %ready) wrapped=cause]]
          =(phase %block)
        ::
        %-  (debug "handle-npc: %block: preparing {<(lent block-commands)>} commands for balance update")
        ::  move each command to balance update phase
        =.  pending-commands.state
          %-  ~(gas z-by:zo pending-commands.state)
          %+  turn  block-commands
          |=  [pid=@ud [phase=?(%block %balance %ready) wrapped=cause]]
          [pid [%balance wrapped]]
        ::  check if we need to update balance (if there are commands waiting for it)
        =/  have-balance-cmds=?
          %-  ~(any z-by:zo pending-commands.state)
          |=  [phase=?(%block %balance %ready) wrapped=*]
          =(phase %balance)
        ::
        ?:  have-balance-cmds
          %-  (debug "handle-npc: %block: initiating balance update for pending commands")
          =^  balance-update-effs  state
            (do-update-balance [%update-balance ~])
          [balance-update-effs state]
        `state
        ::
      ==
    ==
  ::
  ++  handle-pending-commands
    ^-  [(list effect) ^state]
    =/  ready-commands=(list [pid=@ud =cause])
      %+  turn
        %+  skim  ~(tap z-by:zo pending-commands.state)
        |=  [pid=@ud [phase=?(%block %balance %ready) wrapped=cause]]
        =(phase %ready)
      |=  [pid=@ud [phase=?(%block %balance %ready) wrapped=cause]]
      [pid wrapped]
    ?~  ready-commands
      %-  (debug "no pending commands to execute")
      `state
    %-  (debug "executing {<(lent ready-commands)>} pending commands")
    ::  process each command
    =/  effs=(list effect)  ~
    =/  cmds=(list [pid=@ud =cause])  ready-commands
    |-
    ?~  cmds
      [effs state]
    =/  pid=@ud  pid.i.cmds
    =/  =cause  +.i.cmds
    =/  ov=^ovum
      %*  .  ovum
        cause.input  cause
      ==
    =.  pending-commands.state
      (~(del z-by:zo pending-commands.state) pid)
    ::
    =^  cmd-effs  state
      =+  try-poke=(mule |.((poke ov)))
      ?-  -.try-poke
          %|
        ~>  %slog.[%0 leaf+"poke failed, exiting"]
        ((slog p.try-poke) [[%exit 0]~ state])
        %&  p.try-poke
      ==
    $(cmds t.cmds, effs (weld effs cmd-effs))
  ::
  ++  do-sync-run
    |=  =cause
    ?>  ?=(%sync-run -.cause)
    %-  (debug "sync-run: wrapped command {<-.wrapped.cause>}")
    ::  get a new pid for the command
    =/  pid=(unit @ud)  (generate-pid:v %block)
    ?~  pid
      ::  if we can't get a pid, run the command directly
      %-  (debug "sync-run: no pid available, clearing and exiting")
      =.  peek-requests.state  *_peek-requests.state
      [[%exit 0]~ state]
    ::  store the command in pending-commands
    =.  pending-commands.state
      %+  ~(put z-by:zo pending-commands.state)
        u.pid
      [%block wrapped.cause]
    ::  initiate block update
    =^  block-update-effs  state
      (do-update-block [%update-block ~])
    ::  return block update effects
    [block-update-effs state]
  ::
  ++  do-update-balance
    |=  =cause
    ?>  ?=(%update-balance -.cause)
    %-  (debug "update-balance")
    %-  (debug "last balance size: {<(lent ~(tap z-by:zo balance.state))>}")
    =/  pid=(unit @ud)  (generate-pid:v %balance)
    ?~  pid  `state
    =/  bid=block-id:transact
      ?:  ?=(~ last-block.state)
        ~|("no last block found, not updating balance" !!)
      (need last-block.state)
    =/  =path  (snoc /balance (to-b58:block-id:transact bid))
    =/  =effect  [%npc u.pid %peek path]
    :-  ~[effect]
    state(peek-requests (~(put by peek-requests.state) u.pid %balance))
  ::
  ++  do-update-block
    |=  =cause
    ?>  ?=(%update-block -.cause)
    %-  (debug "update-block")
    %-  (debug "last block: {<last-block.state>}")
    =/  pid=(unit @ud)  (generate-pid:v %block)
    ?~  pid  `state
    =/  =path  /heavy
    =/  =effect  [%npc u.pid %peek path]
    :-  ~[effect]
    state(peek-requests (~(put by peek-requests.state) u.pid %block))
  ::
  ++  do-import-keys
    |=  =cause
    ?>  ?=(%import-keys -.cause)
    =/  new-keys=_keys.state
      %+  roll  keys.cause
      |=  [[=trek =meta] acc=_keys.state]
      (~(put of acc) trek meta)
    =/  master-key=coil
      %-  head
      %+  murn  ~(tap of new-keys)
      |=  [t=trek m=meta]
      ^-  (unit coil)
      ?:  ?&
            ?=(%coil -.m)
            =((slag 2 t) /pub/m)
          ==
        `m
      ~
    =/  key-list=(list tape)
      %+  murn  ~(tap of new-keys)
      |=  [t=trek m=meta]
      ^-  (unit tape)
      ?:  ?=(%coil -.m)
        `(en:base58:wrap p.key.m)
      ~
    =.  master.state  `master-key
    :_  state(keys new-keys)
    :~  :-  %markdown
        %-  crip
        """
        ## imported keys

        {<key-list>}
        """
        [%exit 0]
    ==
  ::
  ++  do-import-extended
    |=  =cause
    ?>  ?=(%import-extended -.cause)
    %-  (debug "import-extended: {<extended-key.cause>}")
    =/  core  (from-extended-key:s10 extended-key.cause)
    =/  is-private=?  !=(0 prv:core)
    =/  key-type=?(%pub %prv)  ?:(is-private %prv %pub)
    =/  coil-key=key
      ?:  is-private
        [%prv private-key:core]
      [%pub public-key:core]
    =/  imported-coil=coil  [%coil coil-key chain-code:core]
    =/  public-coil=coil  [%coil [%pub public-key] chain-code]:core
    =/  key-label=@t
      ?:  is-private
        (crip "imported-private-{<(end [3 4] public-key:core)>}")
      (crip "imported-public-{<(end [3 4] public-key:core)>}")
    ::  if this is a master key (no parent), set as master
    ?:  =(0 dep:core)
      =.  master.state  (some public-coil)
      =.  state  set-receive-address:v
      =.  keys.state  (key:put:v imported-coil ~ `key-label)
      =.  keys.state  (key:put:v public-coil ~ `key-label)
      =/  extended-type=tape  ?:(is-private "private" "public")
      :_  state
      :~  :-  %markdown
          %-  crip
          """
          ## imported import {extended-type} key

          - import key: {(trip extended-key.cause)}
          - label: {(trip key-label)}
          - set as master key
          """
          [%exit 0]
      ==
    ::  otherwise, import as derived key
    ::  first validate that this key is actually a child of the current master
    ?~  master.state
      :_  state
      :~  :-  %markdown
          %-  crip
          """
          ## import failed

          cannot import derived key: no master key set
          """
          [%exit 1]
      ==
    =/  master-pubkey-coil=coil  (public:master master.state)
    =/  expected-children=(set coil)
      (derive-child:v ind:core)
    =/  imported-pubkey=@  public-key:core
    ::  find the public key coil from the derived children set
    =/  expected-pubkey-coil=(unit coil)
      %-  ~(rep in expected-children)
      |=  [=coil acc=(unit coil)]
      ?^  acc  acc
      ?:  ?=(%pub -.key.coil)
        `coil
      ~
    ?~  expected-pubkey-coil
      ~|("no public key found in derived children - this should not happen" !!)
    =/  expected-pubkey=@  p.key.u.expected-pubkey-coil
    ?.  =(imported-pubkey expected-pubkey)
      :_  state
      :~  :-  %markdown
          %-  crip
          """
          ## import failed

          imported key at index {<ind:core>} does not match expected child of master key

          - imported pubkey: {<imported-pubkey>}
          - expected pubkey: {<expected-pubkey>}
          """
          [%exit 1]
      ==
    ::  key is valid, proceed with import
    =.  keys.state  (key:put:v imported-coil `ind:core `key-label)
    =/  extended-type=tape  ?:(is-private "private" "public")
    :_  state
    :~  :-  %markdown
        %-  crip
        """
        ## imported import {extended-type} key

        - import key: {(trip extended-key.cause)}
        - label: {(trip key-label)}
        - index: {<ind:core>}
        - verified as child of master key
        """
        [%exit 0]
    ==
  ::
  ++  do-export-keys
    |=  =cause
    ?>  ?=(%export-keys -.cause)
    =/  keys-list=(list [trek meta])
      ~(tap of keys.state)
    =/  dat-jam  (jam keys-list)
    =/  path=@t  'keys.export'
    =/  =effect  [%file %write path dat-jam]
    :_  state
    :~  effect
        :-  %markdown
        %-  crip
        """
        ## exported keys

        - path: {<path>}
        """
        [%exit 0]
    ==
  ::
  ++  do-export-master-pubkey
    |=  =cause
    ?>  ?=(%export-master-pubkey -.cause)
    %-  (debug "export-master-pubkey")
    ?~  master.state
      ~&  "wallet warning: no master keys available for export"
      [[%exit 0]~ state]
    =/  master-coil=coil  ~(master get:v %pub)
    ?.  ?=(%pub -.key.master-coil)
      ~&  "wallet fatal: master pubkey malformed"
      [[%exit 0]~ state]
    =/  dat-jam=@  (jam master-coil)
    =/  key-b58=tape  (en:base58:wrap p.key.master-coil)
    =/  cc-b58=tape  (en:base58:wrap cc.master-coil)
    =/  extended-key=@t
      =/  core  (from-public:s10 [p.key cc]:master-coil)
      extended-public-key:core
    :_  state
    :~  :-  %markdown
        %-  crip
        """
        ## exported master public key:

        - import key: {(trip extended-key)}
        - pubkey: {key-b58}
        - chaincode: {cc-b58}

        """
        [%exit 0]
        [%file %write 'master-pubkey.export' dat-jam]
    ==
  ::
  ++  do-import-master-pubkey
    |=  =cause
    ?>  ?=(%import-master-pubkey -.cause)
    %-  (debug "import-master-pubkey: {<coil.cause>}")
    =/  master-pubkey-coil=coil  coil.cause
    =.  master.state  (some master-pubkey-coil)
    =.  state  set-receive-address:v
    =/  label  `(crip "master-public-{<(end [3 4] p.key.master-pubkey-coil)>}")
    =.  keys.state  (key:put:v master-pubkey-coil ~ label)
    =/  key-b58=tape  (en:base58:wrap p.key.master-pubkey-coil)
    =/  cc-b58=tape  (en:base58:wrap cc.master-pubkey-coil)
    :_  state
    :~  :-  %markdown
        %-  crip
        """
        ## imported master public key:

            - pubkey: {key-b58}
            - chaincode: {cc-b58}
        """
        [%exit 0]
    ==
  ::
  ++  do-gen-master-privkey
    |=  =cause
    ?>  ?=(%gen-master-privkey -.cause)
    ::  We do not need to reverse the endian-ness of the seedphrase
    ::  because the bip39 code expects a tape.
    =/  seed=byts  [64 (to-seed:bip39 (trip seedphrase.cause) "")]
    =/  cor  (from-seed:s10 seed)
    =/  master-pubkey-coil=coil  [%coil [%pub public-key] chain-code]:cor
    =/  master-privkey-coil=coil  [%coil [%prv private-key] chain-code]:cor
    =.  master.state  (some master-pubkey-coil)
    =.  state  set-receive-address:v
    =/  public-label  `(crip "master-public-{<(end [3 4] public-key:cor)>}")
    =/  private-label  `(crip "master-private-{<(end [3 4] public-key:cor)>}")
    =.  keys.state  (key:put:v master-privkey-coil ~ private-label)
    =.  keys.state  (key:put:v master-pubkey-coil ~ public-label)
    =.  keys.state  (seed:put:v seedphrase.cause)
    %-  (debug "master.state: {<master.state>}")
    [[%exit 0]~ state]
  ::
  ++  do-gen-master-pubkey
    |=  =cause
    ?>  ?=(%gen-master-pubkey -.cause)
    =/  privkey-atom=@
      (de:base58:wrap (trip privkey-b58.cause))
    =/  chain-code-atom=@
      (de:base58:wrap (trip cc-b58.cause))
    =/  =keyc:slip10  [privkey-atom chain-code-atom]
    =/  cor  (from-private:s10 keyc)
    =/  master-pubkey-coil=coil  [%coil [%pub public-key] chain-code]:cor
    =/  master-privkey-coil=coil  [%coil [%prv private-key] chain-code]:cor
    %-  (debug "Generated master public key: {<public-key:cor>}")
    =/  public-label  `(crip "master-public-{<(end [3 4] public-key:cor)>}")
    =/  private-label  `(crip "master-private-{<(end [3 4] public-key:cor)>}")
    =.  master.state  (some master-pubkey-coil)
    =.  state  set-receive-address:v
    =.  keys.state  (key:put:v master-privkey-coil ~ private-label)
    =.  keys.state  (key:put:v master-pubkey-coil ~ public-label)
    %-  (debug "master.state: {<master.state>}")
    =/  extended-key=@t  extended-public-key:cor
    :_  state
    :~  :-  %markdown
        %-  crip
        """
        ## master public key (import)

        - import key: {(trip extended-key)}
        - pubkey: {<(en:base58:wrap p.key.master-pubkey-coil)>}
        - chaincode: {<(en:base58:wrap cc.master-pubkey-coil)>}
        """
        [%exit 0]
    ==
  ::
  ++  do-send-tx
    |=  =cause
    ?>  ?=(%send-tx -.cause)
    %-  (debug "send-tx: creating raw-tx")
    ::  note that new:raw-tx calls +validate already
    =/  raw=raw-tx:transact  (new:raw-tx:transact p.dat.cause)
    =/  tx-id  id.raw
    =/  nock-cause=$>(%fact cause:dumb)
      [%fact %0 %heard-tx raw]
    %-  (debug "send-tx: made raw-tx, poking over npc")
    :_  state
    :~
      [%npc 0 %poke nock-cause]
      [%exit 0]
    ==
  ::
  ++  do-list-pubkeys
    |=  =cause
    ?>  ?=(%list-pubkeys -.cause)
    =/  pubkeys  ~(coils get:v %pub)
    =/  base58-keys=(list tape)
      %+  turn  pubkeys
      |=  =coil
      =/  pubkey=schnorr-pubkey:transact  pub:(from-public:s10 [p.key cc]:coil)
      %-  trip
      (to-b58:schnorr-pubkey:transact pubkey)
    :_  state
    :~  :-  %markdown
        %-  crip
        """
        ## pubkeys

        {<base58-keys>}
        """
        [%exit 0]
    ==
  ::
  ++  do-show-seedphrase
    |=  =cause
    ?>  ?=(%show-seedphrase -.cause)
    %-  (debug "show-seedphrase")
    =/  =meta  seed:get:v
    =/  seedphrase=@t
      ?:  ?=(%seed -.meta)
        +.meta
      %-  crip
      "no seedphrase found"
    :_  state
    :~  :-  %markdown
        %-  crip
        """
        ## seedphrase

        {<seedphrase>}
        """
        [%exit 0]
    ==
  ::
  ++  do-show-master-pubkey
    |=  =cause
    ?>  ?=(%show-master-pubkey -.cause)
    %-  (debug "show-master-pubkey")
    =/  =meta  ~(master get:v %pub)
    ?>  ?=(%coil -.meta)
    =/  extended-key=@t
      =/  core  (from-public:s10 [p.key cc]:meta)
      extended-public-key:core
    :_  state
    :~  :-  %markdown
        %-  crip
        """
        ## master public key

        - import key: {(trip extended-key)}
        - pubkey: {<p.key.meta>}
        - chaincode: {<cc.meta>}
        """
        [%exit 0]
    ==
  ::
  ++  do-show-master-privkey
    |=  =cause
    ?>  ?=(%show-master-privkey -.cause)
    %-  (debug "show-master-privkey")
    =/  =meta  ~(master get:v %prv)
    ?>  ?=(%coil -.meta)
    =/  extended-key=@t
      =/  core  (from-private:s10 [p.key cc]:meta)
      extended-private-key:core
    =/  key-b58=tape  (en:base58:wrap p.key.meta)
    =/  cc-b58=tape  (en:base58:wrap cc.meta)
    :_  state
    :~  :-  %markdown
        %-  crip
        """
        ## master private key

        - import key: {(trip extended-key)}
        - private key: {key-b58}
        - chaincode: {cc-b58}
        """
        [%exit 0]
    ==
  ++  do-scan
    |=  =cause
    ?>  ?=(%scan -.cause)
    %-  (debug "scan: scanning {<search-depth.cause>} addresses")
    ?>  ?=(^ master.state)
    ::  get all public keys up to search depth
    =/  index=@ud  search-depth.cause
    =/  coils=(list coil)
      =/  keys=(list coil)  [~(master get:v %pub)]~
      =|  done=_|
      |-  ^-  (list coil)
      ?:  done  keys
      =?  done  =(0 index)  &
      =/  base=trek  /keys/pub/[ux/p.key:(public:master master.state)]/[ud/index]
      =/  key=(unit coil)
        ;;  (unit coil)
        (~(get of keys.state) base)
      %=  $
        index  ?:(=(0 index) 0 (dec index))
        keys  ?^(key (snoc keys u.key) keys)
      ==
    ::  fail when no coils
    ?:  ?=(~ coils)
      ~|("no coils for master key" !!)
    ::  generate first names of notes owned by each pubkey
    =/  first-names=(list [hash:transact schnorr-pubkey:transact])
      %+  turn  coils
      |=  =coil
      ::  create lock from public key
      =/  pubkey=schnorr-pubkey:transact  pub:(from-public:s10 [p.key cc]:coil)
      =/  =lock:transact  (new:lock:transact pubkey)
      ::  generate name and take first name
      =/  match-name=nname:transact
        %-  new:nname:transact
        :*  lock
            [*hash:transact %.n]  ::  empty source, not a coinbase
            *timelock:transact    ::  no timelock
        ==
      [-.match-name pubkey]
    ::  find notes with matching first names in balance
    =/  notes=(list nnote:transact)
      %+  murn
        ~(tap z-by:zo balance.state)
      |=  [name=nname:transact note=nnote:transact]
      ^-  (unit nnote:transact)
      ::  check if first name matches any in our list
      =/  matches
        %+  lien  first-names
        |=  [first-name=hash:transact pubkey=schnorr-pubkey:transact]
        =/  =lock:transact  (new:lock:transact pubkey)
        ::  update lock if include-multisig is true and pubkey is in
        ::  the multisig set in the note's lock
        =?  lock
          ?&  include-multisig.cause
              (~(has z-in:zo pubkeys.lock.note) pubkey)
          ==
        lock.note
        ::  update match-name if include-timelocks is set
        =?  first-name  include-timelocks.cause
          =<  -
          %-  new:nname:transact
          :*  lock
              [*hash:transact %.n]  ::  empty source, not a coinbase
              timelock.note         ::  include timelock
          ==
        =(-.name first-name)
      ?:(matches `note ~)
    %-  (debug "found matches: {<notes>}")
    =/  nodes=markdown:m
      :~  :-  %leaf
          :-  %heading
          :*  %atx  1
              :~  [%text 'Scan Result']
              ==
          ==
          :-  %container
          :-  %ul
          :*  0  '*'
             (turn notes display-note)
          ==
      ==
    :_  state
    ~[(make-markdown-effect nodes) [%exit 0]]
  ::
  ++  do-list-notes
    |=  =cause
    ?>  ?=(%list-notes -.cause)
    %-  (debug "list-notes")
    :_  state
    :~  :-  %markdown
      %-  crip
      %+  welp
      """
      ## wallet notes

      """
      %-  zing
      %+  turn  ~(val z-by:zo balance.state)
      |=  =nnote:transact
      %-  trip
      (display-note-cord nnote)
      ::
      [%exit 0]
    ==
  ::
  ++  do-list-notes-by-pubkey
    |=  =cause
    ?>  ?=(%list-notes-by-pubkey -.cause)
    =/  target-pubkey=schnorr-pubkey:transact
      (from-b58:schnorr-pubkey:transact pubkey.cause)
    =/  matching-notes=(list [name=nname:transact note=nnote:transact])
      %+  skim  ~(tap z-by:zo balance.state)
      |=  [name=nname:transact note=nnote:transact]
      (~(has z-in:zo pubkeys.lock.note) target-pubkey)
    :_  state
    :~  :-  %markdown
        %-  crip
        %+  welp
          """
          ## wallet notes for pubkey {<(to-b58:schnorr-pubkey:transact target-pubkey)>}

          """
        %-  zing
        %+  turn  matching-notes
        |=  [* =nnote:transact]
        %-  trip
        (display-note-cord nnote)
        ::
        [%exit 0]
    ==
  ::
  ++  do-list-notes-by-pubkey-csv
    |=  =cause
    ?>  ?=(%list-notes-by-pubkey-csv -.cause)
    =/  target-pubkey=schnorr-pubkey:transact
      (from-b58:schnorr-pubkey:transact pubkey.cause)
    =/  matching-notes=(list [name=nname:transact note=nnote:transact])
      %+  skim  ~(tap z-by:zo balance.state)
      |=  [name=nname:transact note=nnote:transact]
      (~(has z-in:zo pubkeys.lock.note) target-pubkey)
    =/  csv-header=tape
      "name_first,name_last,assets,block_height,source_hash"
    =/  csv-rows=(list tape)
      %+  turn  matching-notes
      |=  [name=nname:transact note=nnote:transact]
      =/  name-b58=[first=@t last=@t]  (to-b58:nname:transact name)
      =/  source-hash-b58=@t  (to-b58:hash:transact p.source.note)
      """
      {(trip first.name-b58)},{(trip last.name-b58)},{(ui-to-tape assets.note)},{(ui-to-tape origin-page.note)},{(trip source-hash-b58)}
      """
    =/  csv-content=tape
      %+  welp  csv-header
      %+  welp  "\0a"
      %-  zing
      %+  turn  csv-rows
      |=  row=tape
      "{row}\0a"
    =/  filename=@t
      %-  crip
      "notes-{(trip (to-b58:schnorr-pubkey:transact target-pubkey))}.csv"
    :_  state
    :~  :-  %file
        :-  %write
        :-  filename
        %-  crip
        csv-content
        [%exit 0]
    ==
  ::
  ++  do-simple-spend
    |=  =cause
    ?>  ?=(%simple-spend -.cause)
    %-  (debug "simple-spend: {<names.cause>}")
    ::  for now, each input corresponds to a single name and recipient. all
    ::  assets associated with the name are transferred to the recipient.
    ::
    ::  thus there is one recipient per name, and one seed per recipient.
    ::
    =/  names=(list nname:transact)
      %+  turn  names.cause
      |=  [first=@t last=@t]
      (from-b58:nname:transact [first last])
    =/  recipients=(list lock:transact)
      %+  turn  recipients.cause
      |=  [m=@ pks=(list @t)]
      %+  m-of-n:new:lock:transact  m
      %-  ~(gas z-in:zo *(z-set:zo schnorr-pubkey:transact))
      %+  turn  pks
      |=  pk=@t
      (from-b58:schnorr-pubkey:transact pk)
    ::
    =/  gifts=(list coins:transact)  gifts.cause
    ::
    ?.  ?&  =((lent names) (lent recipients))
            =((lent names) (lent gifts))
        ==
      ~|("different number of names/recipients/gifts" !!)
    =|  =ledger
    =.  ledger
      |-
      ?~  names  ledger
      ::  since we assert they are equal in length above, this is just to get
      ::  the i face
      ?~  gifts  ledger
      ?~  recipients  ledger
      %=  $
        ledger      [[i.names i.recipients i.gifts timelock-intent.cause] ledger]
        names       t.names
        gifts       t.gifts
        recipients  t.recipients
      ==
    ::
    ::  the fee is subtracted from the first note that permits doing so without overspending
    =/  fee=coins:transact  fee.cause
    ::  get private key at specified index, or first derived key if no index
    =/  sender=coil
      ?~  index.cause  ~(master get:v %prv)
      =/  key-at-index=meta  (~(by-index get:v %prv) u.index.cause)
      ?>  ?=(%coil -.key-at-index)
      key-at-index
    =/  sender-key=schnorr-seckey:transact
      (from-atom:schnorr-seckey:transact p.key.sender)
    ::  for each name, create an input from the corresponding note in sender's
    ::  balance at the current block. the fee will be subtracted entirely from
    ::  the first note that has sufficient assets for both the fee and the gift.
    ::  the refund is sent to receive-address.state
    =/  [ins=(list input:transact) spent-fee=?]
      %^  spin  ledger  `?`%.n
      |=  $:  $:  name=nname:transact
                  recipient=lock:transact
                  gift=coins:transact
                  =timelock-intent:transact
              ==
            spent-fee=?
          ==
      =/  note=nnote:transact  (get-note:v name)
      ?:  (gth gift assets.note)
        ~|  "gift {<gift>} larger than assets {<assets.note>} for recipient {<recipient>}"
        !!
      ?:  ?&  !spent-fee
              (lte (add gift fee) assets.note)
          ==
        ::  we can subtract the fee from this note
        :_  %.y
        =/  gift-seed=seed:transact
          %-  new:seed:transact
          :*  *(unit source:transact)      :: output-source
              recipient                    :: recipient
              timelock-intent              :: timelock-intent
              gift                         :: gift
              (hash:nnote:transact note)   :: parent-hash
          ==
        =/  refund=coins:transact  (sub assets.note (add gift fee))
        =/  refund-seed=seed:transact
          %-  new:seed:transact
          :*  *(unit source:transact)
              assert-receive-address:v
              *timelock-intent:transact    :: no timelock on refund
              refund
              (hash:nnote:transact note)
          ==
        =/  seed-list=(list seed:transact)
          ?:  =(0 refund)  ~[gift-seed]
          ~[gift-seed refund-seed]
        =/  seeds-set=seeds:transact  (new:seeds:transact seed-list)
        =/  spend-obj=spend:transact  (new:spend:transact seeds-set fee)
        =.  spend-obj  (sign:spend:transact spend-obj sender-key)
        [note spend-obj]
      ::  we cannot subtract the fee from this note, or we already have from a previous one
      :_  spent-fee
      =/  gift-seed=seed:transact
        %-  new:seed:transact
        :*  *(unit source:transact)      :: output-source
            recipient                    :: recipient
            timelock-intent              :: timelock-intent
            gift                         :: gift
            (hash:nnote:transact note)   :: parent-hash
        ==
      =/  seeds-set=seeds:transact  (new:seeds:transact ~[gift-seed])
      =/  spend-obj=spend:transact  (new:spend:transact seeds-set 0)
      =.  spend-obj  (sign:spend:transact spend-obj sender-key)
      [note spend-obj]
    ::
    ?.  spent-fee
      ~|("no note suitable to subtract fee from, aborting operation" !!)
    =/  ins-draft=inputs:transact  (multi:new:inputs:transact ins)
    ?:  ?=(~ last-block.state)
      ~|("last-block unknown!" !!)
    ::  name is the b58-encoded name of the first input
    =/  draft-name=@t
      %-  head
      %+  turn  ~(tap z-by:zo (names:inputs:transact ins-draft))
      |=  =nname:transact
      =<  last
      (to-b58:nname:transact nname)
    ::  jam inputs and save as draft
    =/  =draft
      %*  .  *draft
        p  ins-draft
        name  draft-name
      ==
    =/  draft-jam  (jam draft)
    =/  markdown-text=@t
      %-  crip
      """
      ## draft

      - {<draft-name>}

      {<draft>}
      """
    =/  path=@t
      %-  crip
      "./drafts/{(trip name.draft)}.draft"
    %-  (debug "saving draft to {<path>}")
    =/  =effect  [%file %write path draft-jam]
    :-  ~[effect [%markdown markdown-text] [%exit 0]]
    state
  ::
  ++  do-keygen
    |=  =cause
    ?>  ?=(%keygen -.cause)
    =+  [seed-phrase=@t cor]=(gen-master-key:s10 entropy.cause salt.cause)
    =/  master-public-coil  [%coil [%pub public-key] chain-code]:cor
    =/  master-private-coil  [%coil [%prv private-key] chain-code]:cor
    =.  master.state  (some master-public-coil)
    =.  state  set-receive-address:v
    %-  (debug "keygen: public key: {<(en:base58:wrap public-key:cor)>}")
    %-  (debug "keygen: private key: {<(en:base58:wrap private-key:cor)>}")
    =/  pub-label  `(crip "master-public-{<(end [3 4] public-key:cor)>}")
    =/  prv-label  `(crip "master-public-{<(end [3 4] public-key:cor)>}")
    =.  keys.state  (key:put:v master-public-coil ~ pub-label)
    =.  keys.state  (key:put:v master-private-coil ~ prv-label)
    =.  keys.state  (seed:put:v seed-phrase)
    =/  extended-private=@t  extended-private-key:cor
    =/  extended-public=@t  extended-public-key:cor
    :_  state
    :~  :-  %markdown
        %-  crip
        """
        ## Keygen

        ### New Public Key
        {<(en:base58:wrap public-key:cor)>}

        ### New Private Key
        {<(en:base58:wrap private-key:cor)>}

        ### Chain Code
        {<(en:base58:wrap chain-code:cor)>}

        ### Import Private Key
        {(trip extended-private)}

        ### Import Public Key
        {(trip extended-public)}

        ### Seed Phrase
        {<seed-phrase>}
        """
        [%exit 0]
    ==
  ::
  ::  derives child keys of current master key
  ::  at index `i`. this will overwrite existing paths if
  ::  the master key changes
  ++  do-derive-child
    |=  =cause
    ?>  ?=(%derive-child -.cause)
    =/  index
      ?:  hardened.cause
        (add i.cause (bex 31))
      i.cause
    =/  derived-keys=(set coil)  (derive-child:v index)
    =.  keys.state
      %-  ~(rep in derived-keys)
      |=  [=coil keys=_keys.state]
      =.  keys.state  keys
      (key:put:v coil `index label.cause)
    :-  [%exit 0]~
    state
  ::
  ++  do-sign-tx
    |=  =cause
    ?>  ?=(%sign-tx -.cause)
    %-  (debug "sign-tx: {<dat.cause>}")
    ::  get private key at specified index, or first derived key if no index
    =/  private-keys=(list coil)  ~(coils get:v %prv)
    ?~  private-keys
      ~|("No private keys available for signing" !!)
    =/  sender=coil
      ?~  index.cause  i.private-keys
      =/  key-at-index=meta  (~(by-index get:v %prv) u.index.cause)
      ?>  ?=(%coil -.key-at-index)
      key-at-index
    =/  sender-key=schnorr-seckey:transact
      (from-atom:schnorr-seckey:transact p.key.sender)
    =/  signed-inputs=inputs:transact
      %-  ~(run z-by:zo p.dat.cause)
      |=  =input:transact
      %-  (debug "signing input: {<input>}")
      =.  spend.input
        %+  sign:spend:transact
          spend.input
        sender-key
      input
    =/  signed-draft=draft
      %=  dat.cause
        p  signed-inputs
      ==
    =/  draft-jam  (jam signed-draft)
    =/  path=@t
      %-  crip
      "./drafts/{(trip name.signed-draft)}.draft"
    %-  (debug "saving input draft to {<path>}")
    =/  =effect  [%file %write path draft-jam]
    :-  ~[effect [%exit 0]]
    state
  ::
  ++  do-advanced-spend-seed
    |=  cause=advanced-spend-seed
    ^-  [(list effect) ^state]
    |^
    =?  active-draft.state  ?=(~ active-draft.state)  `*draft-name
    =?  active-seed.state  ?=(~ active-seed.state)  `*seed-name
    ?-  -.cause
      %new  do-new
      %set-name  do-set-name
      %set-source  do-set-source
      %set-recipient  do-set-recipient
      %set-timelock  do-set-timelock
      %set-gift  do-set-gift
      %set-parent-hash  do-set-parent-hash
      %set-parent-hash-from-name  do-set-parent-hash-from-name
      %print-status  do-print-status
    ==
    ::
    ++  do-new
      ?>  ?=([%new *] cause)
      =/  sed=preseed
        %*  .  *preseed
          name  name.cause
        ==
      (write-seed sed)
    ::
    ++  do-set-name
      ?>  ?=([%set-name *] cause)
      =/  pre=(unit preseed)
        (get-seed:p seed-name.cause)
      ?>  ?=(^ pre)
      =.  name.u.pre  new-name.cause
      (write-seed u.pre)
    ::
    ++  do-set-source
      ?>  ?=([%set-source *] cause)
      =/  pre=(unit preseed)
        (get-seed:p seed-name.cause)
      ?>  ?=(^ pre)
      =/  sed=preseed
        ?~  source.cause
          %=  u.pre
            output-source.p  ~
            output-source.q  %.y
          ==
        %=  u.pre
          output-source.p  (some (from-b58:source:transact u.source.cause))
          output-source.q  %.y
        ==
      (write-seed sed)
    ::
    ++  do-set-recipient
      ?>  ?=([%set-recipient *] cause)
      =/  pre=(unit preseed)
        (get-seed:p seed-name.cause)
      ?>  ?=(^ pre)
      =/  recipient=lock:transact
        %+  m-of-n:new:lock:transact  m.recipient.cause
        %-  ~(gas z-in:zo *(z-set:zo schnorr-pubkey:transact))
        (turn pks.recipient.cause from-b58:schnorr-pubkey:transact)
      =/  sed=preseed
        %=  u.pre
          recipient.p  recipient
          recipient.q  %.y
        ==
      (write-seed sed)
    ::
    ++  do-set-timelock
      ?>  ?=([%set-timelock *] cause)
      ::TODO
      !!
    ::
    ++  do-set-gift
      ?>  ?=([%set-gift *] cause)
      =/  pre=(unit preseed)
        (get-seed:p seed-name.cause)
      ?>  ?=(^ pre)
      =/  sed=preseed
        %=  u.pre
          gift.p  gift.cause
          gift.q  %.y
        ==
      (write-seed sed)
    ::
    ++  do-set-parent-hash
      ?>  ?=([%set-parent-hash *] cause)
      =/  pre=(unit preseed)
        (get-seed:p seed-name.cause)
      ?>  ?=(^ pre)
      =/  sed=preseed
        %=  u.pre
          parent-hash.q  %.y
          parent-hash.p  (from-b58:hash:transact parent-hash.cause)
        ==
      (write-seed sed)
    ::
    ++  do-set-parent-hash-from-name
      ?>  ?=([%set-parent-hash-from-name *] cause)
      =/  pre=(unit preseed)
        (get-seed:p seed-name.cause)
      ?>  ?=(^ pre)
      =/  name=nname:transact  (from-b58:nname:transact name.cause)
      =/  not=nnote:transact  (get-note:v name)
      =/  sed=preseed
        %=  u.pre
          parent-hash.p  (hash:nnote:transact not)
          parent-hash.q  %.y
        ==
      (write-seed sed)
    ::
    ++  do-print-status
      ?>  ?=([%print-status *] cause)
      =/  pre=(unit preseed)
        (get-seed:p seed-name.cause)
      ?>  ?=(^ pre)
      =/  output-source-text=tape
        ?:  !output-source.q.u.pre
          "Unset (any output source is OK)"
        <output-source.p.u.pre>
      =/  recipient-text=tape
        ?:  !recipient.q.u.pre
          "Unset"
        <recipient.p.u.pre>
      =/  timelock-text=tape
        ?:  !timelock-intent.q.u.pre
          "Unset (no intent)"
        <timelock-intent.p.u.pre>
      =/  gift-text=tape
        ?:  !gift.q.u.pre
          "Gift: unset (gift must be nonzero)"
        ?:  =(0 gift.p.u.pre)
          """
          Gift: 0 (must be nonzero)
          """
        """
        Gift: {<gift.p.u.pre>}
        """
      =/  status-text=tape
        """
        ## Seed Status

        ### Output Source
        {output-source-text}

        ### Recipient
        {recipient-text}

        ### Timelock Intent
        {timelock-text}

        ### Gift
        {gift-text}
        """
      :_  state
      (print (need (de:md (crip status-text))))
    ::
    ++  write-seed
      |=  sed=preseed
      ^-  [(list effect) ^state]
      =.  active-seed.state  (some name.sed)
      =.  draft-tree.state  (~(add-seed plan draft-tree.state) name.sed sed)
      =^  writes  state  write-draft:d
      [writes state]
    --  ::+do-advanced-spend-seed
  ::
  ++  do-advanced-spend-input
    |=  cause=advanced-spend-input
    ^-  [(list effect) ^state]
    |^
    ?-  -.cause
      %new  do-new
      %set-name  do-set-name
      %add-seed  do-add-seed
      %set-fee   do-set-fee
      %set-note-from-name  do-set-note-from-name
      %set-note-from-hash  do-set-note-from-hash
      %derive-note-from-seeds  do-derive-note-from-seeds
      %remove-seed  do-remove-seed
      %remove-seed-by-hash  do-remove-seed-by-hash
      %print-status  do-print-status
    ==
    ::
    ++  do-new
      ?>  ?=([%new *] cause)
      =/  inp=preinput
        %*  .  *preinput
          name  name.cause
        ==
      =.  active-input.state  (some name.cause)
      (write-input inp)
    ::
    ++  do-set-name
      ?>  ?=([%set-name *] cause)
      =/  pre=(unit preinput)
        (get-input:p input-name.cause)
      ?>  ?=(^ pre)
      =.  name.u.pre  new-name.cause
      =.  active-input.state  (some new-name.cause)
      (write-input u.pre)
    ::
    ++  do-add-seed
      ?>  ?=([%add-seed *] cause)
      ::
      =/  pre=(unit preinput)
        (get-input:p input-name.cause)
      ?>  ?=(^ pre)
      =/  sed=(unit preseed)
        (get-seed:p seed-name.cause)
      ?>  ?=(^ sed)
      ?:  (~(has z-in:zo seeds.spend.p.u.pre) p.u.sed)
        :_  state
        =/  nodes=markdown:m
          :~  :-  %leaf
              :-  %paragraph
              :~  [%text (crip "seed already exists in .spend, doing nothing.")]
              ==
          ==
        (print nodes)
      =/  inp=preinput
        %=  u.pre
          seeds.spend.p  (~(put z-in:zo seeds.spend.p.u.pre) p.u.sed)
          seeds.spend.q  %.y
        ==
      (write-input inp)
    ::
    ++  do-set-fee
      ?>  ?=([%set-fee *] cause)
      =/  pre=(unit preinput)
        (get-input:p input-name.cause)
      ?>  ?=(^ pre)
      =.  fee.spend.p.u.pre  fee.cause
      =.  fee.spend.q.u.pre  %.y
      (write-input u.pre)
    ::
    ++  do-set-note-from-name
      ?>  ?=([%set-note-from-name *] cause)
      ::
      =/  pre=(unit preinput)
        (get-input:p input-name.cause)
      ?>  ?=(^ pre)
      =/  name=nname:transact  (from-b58:nname:transact name.cause)
      =/  not=nnote:transact  (get-note:v name)
      =/  inp=preinput
        %=  u.pre
          note.p  not
          note.q  %.y
        ==
      (write-input inp)
    ::
    ++  do-set-note-from-hash
      ?>  ?=([%set-note-from-hash *] cause)
      ::
      =/  pre=(unit preinput)
        (get-input:p input-name.cause)
      ?>  ?=(^ pre)
      =/  =hash:transact  (from-b58:hash:transact hash.cause)
      =/  note=nnote:transact  (get-note-from-hash:v hash)
      =/  inp=preinput
        %=  u.pre
          note.p  note
          note.q  %.y
        ==
      (write-input inp)
    ::
    ++  do-derive-note-from-seeds
      ?>  ?=([%derive-note-from-seeds *] cause)
      ::
      =/  pre=(unit preinput)
        (get-input:p input-name.cause)
      ?>  ?=(^ pre)
      =/  seeds-list=(list seed:transact)
        ~(tap z-in:zo seeds.spend.p.u.pre)
      ?~  seeds-list
        :_  state
        =/  nodes=markdown:m
          :~  :-  %leaf
              :-  %paragraph
              :~  [%text (crip "no seeds exist in .spend, so note cannot be set.")]
              ==
          ==
        (print nodes)
      =/  =hash:transact  parent-hash.i.seeds-list
      =/  note=nnote:transact  (get-note-from-hash:v hash)
      =/  inp=preinput
        %=  u.pre
          note.p  note
          note.q  %.y
        ==
      (write-input inp)
    ::
    ++  do-remove-seed
      ?>  ?=([%remove-seed *] cause)
      ::
      =/  pre=(unit preinput)
        (get-input:p input-name.cause)
      ?>  ?=(^ pre)
      =.  active-input.state  (some input-name.cause)
      =/  sed=(unit preseed)
        (get-seed:p seed-name.cause)
      ?>  ?=(^ sed)
      ?:  !(~(has z-in:zo seeds.spend.p.u.pre) p.u.sed)
        :_  state
        =/  nodes=markdown:m
          :~  :-  %leaf
              :-  %paragraph
              :~  [%text (crip "seed does not exist in .spend, doing nothing")]
              ==
          ==
        (print nodes)
      =/  inp=preinput
        %=  u.pre
          seeds.spend.p  (~(del z-in:zo seeds.spend.p.u.pre) p.u.sed)
          seeds.spend.q  !=(*seeds:transact seeds.spend.p.u.pre)
        ==
      (write-input inp)
    ::
    ++  do-remove-seed-by-hash
      ?>  ?=([%remove-seed-by-hash *] cause)
      :: find seed with hash
      =/  pre=(unit preinput)
        (get-input:p input-name.cause)
      ?>  ?=(^ pre)
      =.  active-input.state  (some input-name.cause)
      =/  seed-hashes=(z-map:zo hash:transact seed:transact)
        %-  ~(gas z-by:zo *(z-map:zo hash:transact seed:transact))
        %+  turn  ~(tap z-in:zo seeds.spend.p.u.pre)
        |=  sed=seed:transact
        [(hash:seed:transact sed) sed]
      =/  has=hash:transact  (from-b58:hash:transact hash.cause)
      ?.  (~(has z-by:zo seed-hashes) has)
        :_  state
        =/  nodes=markdown:m
          :~  :-  %leaf
              :-  %paragraph
              :~  [%text (crip "seed does not exist in .spend, doing nothing")]
              ==
          ==
        (print nodes)
      =/  remove-seed=seed:transact
        (~(got z-by:zo seed-hashes) has)
      ::
      =/  inp=preinput
        %=  u.pre
          seeds.spend.p  (~(del z-in:zo seeds.spend.p.u.pre) remove-seed)
          seeds.spend.q  !=(*seeds:transact seeds.spend.p.u.pre)
        ==
      (write-input inp)
    ::
    ++  do-print-status
      ?>  ?=([%print-status *] cause)
      =/  pre=(unit preinput)
        (get-input:p input-name.cause)
      ?>  ?=(^ pre)
      =|  status-nodes=markdown:m
      =.  status-nodes
        %+  snoc  status-nodes
        :-  %leaf
        :-  %paragraph
        ?:  !signature.spend.q.u.pre
          ::  TODO we removed the ability to sign in this control flow
          [%text (crip ".signature: unset")]~
        ::  check the signature
        ::
        ::  get a .parent-hash of a seed. they have to all be the same, so which
        ::  one doesn't matter; if they're not all the same validation will fail.
        =/  seeds-list=(list seed:transact)
          ~(tap z-in:zo seeds.spend.p.u.pre)
        ?~  seeds-list
          [%text (crip "no seeds exist, so signature cannot be checked")]~
        =/  parent-note-hash=hash:transact  parent-hash.i.seeds-list
        =/  parent-note-hash-b58=tape
          (trip (to-b58:hash:transact parent-note-hash))
        =/  parent-note-name=(unit nname:transact)
          (find-name-by-hash:v parent-note-hash)
        ?~  parent-note-name
          :~
            :-  %text
            %-  crip
            """
            note with hash {parent-note-hash-b58} present in .spend but
            has no matching .name in wallet
            """
            :-  %text
            ::  TODO better, more succint error message.
            '''
            this implies that it is not in the balance unless there is a hash collision.
            please report this as a bug if you are sure you have the $note, as this
            situation is very unlkely. the spend ought to still be valid in that case
            and you can broadcast it anyway.
            '''
          ==
        =/  parent-note-name-b58=tape
          =;  [first=@t last=@t]
            "<(trip first)> <(trip last)>"
          (to-b58:nname:transact u.parent-note-name)
        =/  parent-note=(unit nnote:transact)
          (~(get z-by:zo balance.state) u.parent-note-name)
        ?~  parent-note
          :~
            :-  %text
            %-  crip
            """
            note with name {parent-note-name-b58} and hash {parent-note-hash-b58}
            present in .spend but not in balance
            """
          ==
        ?:  (verify:spend:transact spend.p.u.pre u.parent-note)
          [%text (crip "signature(s) on spend are valid.")]~
        ::  missing or invalid sigs
        =/  have-sigs
          %+  turn
            %~  tap  z-in:zo
            ^-  (z-set:zo schnorr-pubkey:transact)
            %~  key  z-by:zo  (need signature.spend.p.u.pre)
          |=  pk=schnorr-pubkey:transact
          [%text (to-b58:schnorr-pubkey:transact pk)]
        ?~  have-sigs
          [%text 'no signatures found!']~
        =/  lock-b58=[m=@ pks=(list @t)]
          (to-b58:lock:transact recipient.i.seeds-list)
        =/  need-sigs
          (turn pks.lock-b58 (lead %text))
        ?~  need-sigs
          [%text 'no recipients found!']~
        ;:  welp
          :~  [%text (crip "signature on spend did not validate.")]
              [%text (crip "signatures on spend:")]
          ==
          ::TODO check if any particular signature did not validate
          have-sigs
          :~  [%text (crip ".lock on parent note:")]
              [%text (crip "number of sigs required: {(scow %ud m.lock-b58)}")]
              [%text (crip "pubkeys of possible signers:")]
          ==
          need-sigs
        ==
      ::TODO  check individual seeds? this would require some refactoring and
      ::the happy path does not involve adding unfinished seeds to an input.
      :_  state
      (print status-nodes)
    ::
    ++  write-input
      |=  inp=preinput
      ^-  [(list effect) ^state]
      =.  active-input.state  (some name.inp)
      =.  draft-tree.state  (~(add-input plan draft-tree.state) name.inp inp)
      =^  writes  state  write-draft:d
      [writes state]
    --  ::+do-advanced-spend-input
  ::
  ++  do-advanced-spend-draft
    |=  cause=advanced-spend-draft
    ^-  [(list effect) ^state]
    |^
    =?  active-draft.state  ?=(~ active-draft.state)  `*draft-name
    ?-  -.cause
      %new  do-new
      %set-name  do-set-name
      %add-input  do-add-input
      %remove-input  do-remove-input
      %remove-input-by-name  do-remove-input-by-name
      %print-status  do-print-status
    ==
    ::
    ++  do-new
      ?>  ?=([%new *] cause)
      =.  active-draft.state  (some name.cause)
      =/  dat=draft
        %*  .  *draft
          name  name.cause
        ==
      (write-draft dat)
    ::
    ++  do-set-name
      ?>  ?=([%set-name *] cause)
      =/  pre=(unit draft)
        (get-draft:p draft-name.cause)
      ?>  ?=(^ pre)
      =.  active-draft.state  (some new-name.cause)
      =.  name.u.pre  new-name.cause
      (write-draft u.pre)
    ::
    ++  do-add-input
      ?>  ?=([%add-input *] cause)
      =/  pre=(unit draft)
        (get-draft:p draft-name.cause)
      ?>  ?=(^ pre)
      =.  active-draft.state  (some draft-name.cause)
      =/  inp=(unit preinput)
        (get-input:p input-name.cause)
      ?>  ?=(^ inp)
      ?:  (~(has z-by:zo p.u.pre) name.note.p.u.inp)
        :_  state
        %-  print
        ^-  markdown:m
        :_  ~  :-  %leaf
        :-  %paragraph
        :_  ~  :-  %text
        %-  crip
        """
        draft already has input with note name
        {<(to-b58:nname:transact name.note.p.u.inp)>}, doing nothing.
        """
      =.  p.u.pre
        (~(put z-by:zo p.u.pre) [name.note.p.u.inp p.u.inp])
      (write-draft u.pre)
    ::
    ++  do-remove-input
      ?>  ?=([%remove-input *] cause)
      =/  pre=(unit draft)
        (get-draft:p draft-name.cause)
      ?>  ?=(^ pre)
      =.  active-draft.state  (some draft-name.cause)
      =/  inp=(unit preinput)
        (get-input:p input-name.cause)
      ?>  ?=(^ inp)
      ?.  (~(has z-by:zo p.u.pre) name.note.p.u.inp)
        :_  state
        %-  print
        :_  ~  :-  %leaf
        :-  %paragraph
        :_  ~  :-  %text
        %-  crip
        """
        draft does not have input with note name
        {<(to-b58:nname:transact name.note.p.u.inp)>}, doing nothing.
        """
      ?.  =(u.inp (~(got z-by:zo p.u.pre) name.note.p.u.inp))
        :_  state
        %-  print
        :_  ~  :-  %leaf
        :-  %paragraph
        :_  ~  :-  %text
        %-  crip
        """
        draft has input with note name
        {<(to-b58:nname:transact name.note.p.u.inp)>}, but it is
        a different input. to remove this input, use %remove-input-by-name
        instead.
        """
      =.  p.u.pre
        (~(del z-by:zo p.u.pre) name.note.p.u.inp)
      (write-draft u.pre)
    ::
    ++  do-remove-input-by-name
      ?>  ?=([%remove-input-by-name *] cause)
      =/  pre=(unit draft)
        (get-draft:p draft-name.cause)
      =.  active-draft.state  (some draft-name.cause)
      ?>  ?=(^ pre)
      =/  name=nname:transact  (from-b58:nname:transact name.cause)
      ?.  (~(has z-by:zo p.u.pre) name)
        :_  state
        %-  print
        :_  ~  :-  %leaf
        :-  %paragraph
        :_  ~  :-  %text
        %-  crip
        """
        draft does not have input with note name {(trip first.name.cause)}
        {(trip last.name.cause)}, doing nothing.
        """
      =.  p.u.pre  (~(del z-by:zo p.u.pre) name)
      (write-draft u.pre)
    ::
    ++  do-print-status
      ?>  ?=([%print-status *] cause)
      =/  pre=(unit draft)
        (get-draft:p draft-name.cause)
      =.  active-draft.state  (some draft-name.cause)
      ?>  ?=(^ pre)
      =/  inputs=(list [name=nname:transact =input:transact])
        ~(tap z-by:zo p.u.pre)
      =/  input-texts=(list tape)
        %+  turn  inputs
        |=  [name=nname:transact =input:transact]
        =/  signature-text=tape  ?~(signature.spend.input "unset" "set")
        =/  name-text=tape  <(to-b58:nname:transact name)>
        =/  note-text=tape  <(to-b58:hash:transact (hash:nnote:transact note.input))>
        =/  seeds-text=tape
          %-  zing
          %+  turn  ~(tap z-in:zo seeds.spend.input)
          |=  =seed:transact
          """
          - recipient: {<(to-b58:lock:transact recipient.seed)>}
          - gift: {<gift.seed>}
          - parent hash: {<(to-b58:hash:transact parent-hash.seed)>}
          """
        """
        #### Input {name-text}:

        - Note hash: {note-text}
        - Fee: {<fee.spend.input>}
        - Signature: {signature-text}

        ##### Seeds

        {seeds-text}

        """
      =/  status-text=tape
        """
        ## Draft Status

        Name: {(trip name.u.pre)}
        Number of inputs: {<(lent inputs)>}

        ### Inputs

        {(zing input-texts)}
        """
      :_  state
      (print (need (de:md (crip status-text))))
    ::
    ++  write-draft
      |=  dat=draft
      ^-  [(list effect) ^state]
      =.  active-draft.state  (some name.dat)
      write-draft:d
    --  ::+do-advanced-spend-draft
  --  ::+poke
--
