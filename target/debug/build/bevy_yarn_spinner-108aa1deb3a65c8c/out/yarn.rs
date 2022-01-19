/// A complete Yarn program.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Program {
    /// The name of the program.
    #[prost(string, tag="1")]
    pub name: ::prost::alloc::string::String,
    /// The collection of nodes in this program.
    #[prost(map="string, message", tag="2")]
    pub nodes: ::std::collections::HashMap<::prost::alloc::string::String, Node>,
}
/// A collection of instructions
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Node {
    /// The name of this node.
    #[prost(string, tag="1")]
    pub name: ::prost::alloc::string::String,
    /// The list of instructions in this node.
    #[prost(message, repeated, tag="2")]
    pub instructions: ::prost::alloc::vec::Vec<Instruction>,
    /// A jump table, mapping the names of labels to positions in the
    /// instructions list.
    #[prost(map="string, int32", tag="3")]
    pub labels: ::std::collections::HashMap<::prost::alloc::string::String, i32>,
    /// The tags associated with this node.
    #[prost(string, repeated, tag="4")]
    pub tags: ::prost::alloc::vec::Vec<::prost::alloc::string::String>,
    /// the entry in the program's string table that contains the original
    /// text of this node; null if this is not available    
    #[prost(string, tag="5")]
    pub source_text_string_id: ::prost::alloc::string::String,
}
/// A single Yarn instruction.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Instruction {
    /// The operation that this instruction will perform.
    #[prost(enumeration="instruction::OpCode", tag="1")]
    pub opcode: i32,
    /// The list of operands, if any, that this instruction uses.
    #[prost(message, repeated, tag="2")]
    pub operands: ::prost::alloc::vec::Vec<Operand>,
}
/// Nested message and enum types in `Instruction`.
pub mod instruction {
    /// The type of instruction that this is.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
    #[repr(i32)]
    pub enum OpCode {
        /// Jumps to a named position in the node.
        /// opA = string: label name
        JumpTo = 0,
        /// Peeks a string from stack, and jumps to that named position in
        /// the node. 
        /// No operands.
        Jump = 1,
        /// Delivers a string ID to the client.
        /// opA = string: string ID
        RunLine = 2,
        /// Delivers a command to the client.
        /// opA = string: command text
        RunCommand = 3,
        /// Adds an entry to the option list (see ShowOptions).
        /// opA = string: string ID for option to add
        AddOption = 4,
        /// Presents the current list of options to the client, then clears
        /// the list. The most recently selected option will be on the top
        /// of the stack when execution resumes.
        /// No operands.
        ShowOptions = 5,
        /// Pushes a string onto the stack.
        /// opA = string: the string to push to the stack.
        PushString = 6,
        /// Pushes a floating point number onto the stack.
        /// opA = float: number to push to stack
        PushFloat = 7,
        /// Pushes a boolean onto the stack.
        /// opA = bool: the bool to push to stack
        PushBool = 8,
        /// Pushes a null value onto the stack.
        /// No operands.
        PushNull = 9,
        /// Jumps to the named position in the the node, if the top of the
        /// stack is not null, zero or false.
        /// opA = string: label name 
        JumpIfFalse = 10,
        /// Discards top of stack.
        /// No operands.
        Pop = 11,
        // Calls a function in the client. Pops as many arguments as the
        // client indicates the function receives, and the result (if any)
        // is pushed to the stack.

        /// opA = string: name of the function
        CallFunc = 12,
        /// Pushes the contents of a variable onto the stack.
        /// opA = name of variable 
        PushVariable = 13,
        /// Stores the contents of the top of the stack in the named
        /// variable. 
        /// opA = name of variable
        StoreVariable = 14,
        /// Stops execution of the program.
        /// No operands.
        Stop = 15,
        /// Run the node whose name is at the top of the stack.
        /// No operands.
        RunNode = 16,
    }
}
/// A value used by an Instruction.
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Operand {
    /// The type of operand this is.
    #[prost(oneof="operand::Value", tags="1, 2, 3")]
    pub value: ::core::option::Option<operand::Value>,
}
/// Nested message and enum types in `Operand`.
pub mod operand {
    /// The type of operand this is.
    #[derive(Clone, PartialEq, ::prost::Oneof)]
    pub enum Value {
        /// A string.
        #[prost(string, tag="1")]
        StringValue(::prost::alloc::string::String),
        /// A boolean (true or false).
        #[prost(bool, tag="2")]
        BoolValue(bool),
        /// A floating point number.
        #[prost(float, tag="3")]
        FloatValue(f32),
    }
}
